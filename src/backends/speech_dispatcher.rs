use std::{
    collections::VecDeque,
    fmt, io,
    ops::{Deref, DerefMut},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    thread,
    time::Duration,
};

use oxilangtag::LanguageTag;
use parking_lot::{Mutex, MutexGuard};
use ssip_client_async::{
    Client, ClientError, ClientName, ClientScope, EVENT_BEGIN, EVENT_CANCELED, EVENT_END,
    EVENT_INDEX_MARK, EVENT_PAUSED, EVENT_RESUMED, EventId, EventType, MessageScope,
    NotificationType, OK_MESSAGE_QUEUED, Priority, PunctuationMode, Request, Response,
    fifo::synchronous::{Builder, UnixStream},
};
use tracing::{info_span, instrument, trace, warn};

use crate::{Backend, BackendId, Callbacks, Error, Features, UtteranceId, Voice};

const POLL_INTERVAL: Duration = Duration::from_millis(10);

const COMMAND_BACKOFF: Duration = Duration::from_millis(1);

const RESPONSE_RETRIES: usize = 200;

/// Utterance lifecycle, for attributing notifications whose id was lost.
#[derive(Debug, Default)]
struct State {
    /// Not yet begun, oldest first.
    queued: VecDeque<u64>,
    active: Option<u64>,
    speaking: bool,
}

struct Connection {
    client: Mutex<Client<UnixStream>>,
    state: Mutex<State>,
    events: Sender<(EventType, u64)>,
    /// Nonzero while commands want the client; the reader stays off the socket.
    commands_waiting: AtomicUsize,
    client_id: u32,
}

struct CommandClient<'a> {
    client: MutexGuard<'a, Client<UnixStream>>,
    commands_waiting: &'a AtomicUsize,
}

impl Deref for CommandClient<'_> {
    type Target = Client<UnixStream>;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

impl DerefMut for CommandClient<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.client
    }
}

impl Drop for CommandClient<'_> {
    fn drop(&mut self) {
        self.commands_waiting.fetch_sub(1, Ordering::Relaxed);
    }
}

impl Connection {
    fn lock_for_command(&self) -> CommandClient<'_> {
        self.commands_waiting.fetch_add(1, Ordering::Relaxed);
        CommandClient {
            client: self.client.lock(),
            commands_waiting: &self.commands_waiting,
        }
    }

    fn transact(
        &self,
        client: &mut Client<UnixStream>,
        request: Request,
    ) -> Result<Response, Error> {
        client.send(request)?;
        receive_response(client, |ntype, message_id| {
            self.handle_event(ntype, message_id);
        })
    }

    fn command(&self, request: Request) -> Result<(), Error> {
        let mut client = self.lock_for_command();
        self.transact(&mut client, request)?;
        Ok(())
    }

    fn get_value(&self, request: Request) -> Result<f32, Error> {
        let mut client = self.lock_for_command();
        match self.transact(&mut client, request)? {
            Response::Get(value) => value.parse().map_err(|_| Error::NoneError),
            _ => Err(Error::NoneError),
        }
    }

    /// `receive_lines` is the only event-tolerant receive that keeps the
    /// message id, but a notification it consumes survives only as a status
    /// code; the affected utterance is inferred from lifecycle order,
    /// unambiguous at a single priority.
    fn receive_message_id(&self, client: &mut Client<UnixStream>) -> Result<u64, Error> {
        let mut retries = RESPONSE_RETRIES;
        loop {
            match client.receive_lines(OK_MESSAGE_QUEUED) {
                Ok(lines) => {
                    return lines
                        .first()
                        .and_then(|line| line.parse().ok())
                        .ok_or(Error::NoneError);
                }
                Err(ClientError::UnexpectedStatus(code))
                    if (EVENT_INDEX_MARK..=EVENT_RESUMED).contains(&code) =>
                {
                    if let Some((ntype, id)) = self.infer_swallowed_event(code) {
                        self.handle_event(ntype, &id.to_string());
                    } else {
                        warn!(code, "Dropped a notification with no matching utterance");
                    }
                }
                Err(ClientError::Io(e)) if is_timeout(&e) => {
                    retries -= 1;
                    if retries == 0 {
                        return Err(response_timeout());
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Attributes a notification known only by status code to the sole
    /// utterance it could concern.
    fn infer_swallowed_event(&self, code: u16) -> Option<(EventType, u64)> {
        let state = self.state.lock();
        match code {
            EVENT_BEGIN => state
                .queued
                .front()
                .copied()
                .map(|id| (EventType::Begin, id)),
            EVENT_END => state.active.map(|id| (EventType::End, id)),
            EVENT_CANCELED => state
                .active
                .or_else(|| state.queued.front().copied())
                .map(|id| (EventType::Cancel, id)),
            EVENT_PAUSED => state.active.map(|id| (EventType::Pause, id)),
            EVENT_RESUMED => state.active.map(|id| (EventType::Resume, id)),
            // Index marks carry no lifecycle state to recover.
            _ => None,
        }
    }

    fn handle_event(&self, ntype: EventType, message_id: &str) {
        let Ok(id) = message_id.parse::<u64>() else {
            warn!(message_id, "Ignoring event with unparseable message id");
            return;
        };
        {
            let mut state = self.state.lock();
            match &ntype {
                EventType::Begin => {
                    state.queued.retain(|queued| *queued != id);
                    state.active = Some(id);
                    state.speaking = true;
                }
                EventType::End => {
                    if state.active == Some(id) {
                        state.active = None;
                    }
                    state.speaking = false;
                }
                EventType::Cancel => {
                    state.queued.retain(|queued| *queued != id);
                    if state.active == Some(id) {
                        state.active = None;
                    }
                    state.speaking = false;
                }
                EventType::Pause => state.speaking = false,
                EventType::Resume => state.speaking = true,
                EventType::IndexMark(_) => return,
            }
        }
        let _ = self.events.send((ntype, id));
    }
}

/// Reads until a response arrives, passing interleaved notifications to
/// `on_event`.
fn receive_response(
    client: &mut Client<UnixStream>,
    mut on_event: impl FnMut(EventType, &str),
) -> Result<Response, Error> {
    let mut retries = RESPONSE_RETRIES;
    loop {
        match client.receive() {
            Ok(response) => match event_from_response(response) {
                Ok((ntype, id)) => on_event(ntype, &id.message),
                Err(response) => return Ok(response),
            },
            Err(ClientError::Io(e)) if is_timeout(&e) => {
                retries -= 1;
                if retries == 0 {
                    return Err(response_timeout());
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
}

fn event_from_response(response: Response) -> Result<(EventType, EventId), Response> {
    match response {
        Response::EventBegin(id) => Ok((EventType::Begin, id)),
        Response::EventEnd(id) => Ok((EventType::End, id)),
        Response::EventCanceled(id) => Ok((EventType::Cancel, id)),
        Response::EventPaused(id) => Ok((EventType::Pause, id)),
        Response::EventResumed(id) => Ok((EventType::Resume, id)),
        Response::EventIndexMark(id, mark) => Ok((EventType::IndexMark(mark), id)),
        other => Err(other),
    }
}

fn is_timeout(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

fn response_timeout() -> Error {
    Error::Io(io::Error::new(
        io::ErrorKind::TimedOut,
        "timed out awaiting a response from Speech Dispatcher",
    ))
}

/// Doubles leading dots so no data line reads as the end-of-data marker.
fn data_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| {
            if line.starts_with('.') {
                format!(".{line}")
            } else {
                line.to_string()
            }
        })
        .collect()
}

/// Sole dispatcher of callbacks, so user code in a callback can call back
/// into the backend.
fn reader_loop(
    connection: &Connection,
    callbacks: &Arc<Mutex<Callbacks>>,
    events: &Receiver<(EventType, u64)>,
    shutdown: &AtomicBool,
) {
    while !shutdown.load(Ordering::Relaxed) {
        while let Ok((ntype, id)) = events.try_recv() {
            dispatch(callbacks, &ntype, id);
        }
        if connection.commands_waiting.load(Ordering::Relaxed) > 0 {
            thread::sleep(COMMAND_BACKOFF);
            continue;
        }
        let mut client = connection.client.lock();
        match client.receive() {
            Ok(response) => match event_from_response(response) {
                Ok((ntype, id)) => connection.handle_event(ntype, &id.message),
                Err(response) => {
                    warn!(?response, "Unexpected response with no command in flight");
                }
            },
            Err(ClientError::Io(e)) if is_timeout(&e) => {}
            Err(e) => {
                warn!(error = %e, "Connection failed, stopping notification dispatch");
                return;
            }
        }
    }
}

fn dispatch(callbacks: &Mutex<Callbacks>, ntype: &EventType, id: u64) {
    let utterance_id = UtteranceId::SpeechDispatcher(id);
    match ntype {
        EventType::Begin => callbacks.lock().utterance_begin(utterance_id),
        EventType::End => callbacks.lock().utterance_end(utterance_id),
        EventType::Cancel => callbacks.lock().utterance_stop(utterance_id),
        EventType::Pause => trace!(id, "Speech paused"),
        EventType::Resume => trace!(id, "Speech resumed"),
        EventType::IndexMark(_) => {}
    }
}

/// Setup-time request; no utterance exists yet, so no notification can
/// interleave.
fn setup_request(client: &mut Client<UnixStream>, request: Request) -> Result<Response, Error> {
    client.send(request)?;
    receive_response(client, |ntype, message_id| {
        warn!(
            ?ntype,
            message_id, "Unexpected event during connection setup"
        );
    })
}

/// Stops the reader thread once every clone of the backend is gone.
struct ReaderShutdown(Arc<AtomicBool>);

impl Drop for ReaderShutdown {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

#[derive(Clone)]
pub(crate) struct SpeechDispatcher {
    connection: Arc<Connection>,
    _shutdown: Arc<ReaderShutdown>,
}

impl fmt::Debug for SpeechDispatcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpeechDispatcher")
            .field("client_id", &self.connection.client_id)
            .finish_non_exhaustive()
    }
}

impl SpeechDispatcher {
    #[instrument(level = "info", skip(callbacks), err)]
    pub(crate) fn new(callbacks: &Arc<Mutex<Callbacks>>) -> std::result::Result<Self, Error> {
        let mut client = Builder::new().timeout(POLL_INTERVAL).build()?;
        setup_request(
            &mut client,
            Request::SetName(ClientName::with_component("tts", "tts", "tts")),
        )?;
        // ssip 0.5.0 (pinned) serializes `Progress` as "important".
        setup_request(&mut client, Request::SetPriority(Priority::Progress))?;
        for ntype in [
            NotificationType::Begin,
            NotificationType::End,
            NotificationType::Cancel,
            NotificationType::Pause,
            NotificationType::Resume,
        ] {
            setup_request(&mut client, Request::SetNotification(ntype, true))?;
        }
        let Response::HistoryClientIdSent(client_id) =
            setup_request(&mut client, Request::HistoryGetClientId)?
        else {
            return Err(Error::NoneError);
        };
        let (events, receiver) = channel();
        let connection = Arc::new(Connection {
            client: Mutex::new(client),
            state: Mutex::default(),
            events,
            commands_waiting: AtomicUsize::new(0),
            client_id,
        });
        let shutdown = Arc::new(AtomicBool::new(false));
        // Entered on the reader thread to connect notifications to their backend.
        let span = info_span!("speech_dispatcher", client_id);
        thread::Builder::new()
            .name("tts-speech-dispatcher".into())
            .spawn({
                let connection = connection.clone();
                let callbacks = callbacks.clone();
                let shutdown = shutdown.clone();
                move || {
                    let _entered = span.enter();
                    reader_loop(&connection, &callbacks, &receiver, &shutdown);
                }
            })?;
        Ok(Self {
            connection,
            _shutdown: Arc::new(ReaderShutdown(shutdown)),
        })
    }
}

impl Backend for SpeechDispatcher {
    #[instrument(level = "trace", skip(self))]
    fn id(&self) -> Option<BackendId> {
        Some(BackendId::SpeechDispatcher(
            self.connection.client_id as usize,
        ))
    }

    #[instrument(level = "trace", skip(self))]
    fn supported_features(&self) -> Features {
        Features {
            stop: true,
            rate: true,
            pitch: true,
            volume: true,
            is_speaking: true,
            voice: true,
            get_voice: false,
            utterance_callbacks: true,
        }
    }

    #[instrument(level = "debug", skip(self), err)]
    fn speak(&mut self, text: &str, interrupt: bool) -> Result<Option<UtteranceId>, Error> {
        if interrupt {
            self.stop()?;
        }
        let mut client = self.connection.lock_for_command();
        let single_char = text.len() == 1;
        if single_char {
            self.connection.transact(
                &mut client,
                Request::SetPunctuationMode(ClientScope::Current, PunctuationMode::All),
            )?;
        }
        self.connection.transact(&mut client, Request::Speak)?;
        client.send(Request::SendLines(data_lines(text)))?;
        let id = self.connection.receive_message_id(&mut client)?;
        self.connection.state.lock().queued.push_back(id);
        if single_char {
            self.connection.transact(
                &mut client,
                Request::SetPunctuationMode(ClientScope::Current, PunctuationMode::None),
            )?;
        }
        Ok(Some(UtteranceId::SpeechDispatcher(id)))
    }

    #[instrument(level = "debug", skip(self), err)]
    fn stop(&mut self) -> Result<(), Error> {
        // `MessageScope::Last` is "self" on the wire: all of this connection's messages.
        self.connection.command(Request::Cancel(MessageScope::Last))
    }

    #[instrument(level = "trace", skip(self))]
    fn min_rate(&self) -> f32 {
        -100.
    }

    #[instrument(level = "trace", skip(self))]
    fn max_rate(&self) -> f32 {
        100.
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_rate(&self) -> f32 {
        0.
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_rate(&self) -> Result<f32, Error> {
        self.connection.get_value(Request::GetRate)
    }

    #[allow(clippy::cast_possible_truncation)]
    #[instrument(level = "debug", skip(self), err)]
    fn set_rate(&mut self, rate: f32) -> Result<(), Error> {
        self.connection
            .command(Request::SetRate(ClientScope::Current, rate as i8))
    }

    #[instrument(level = "trace", skip(self))]
    fn min_pitch(&self) -> f32 {
        -100.
    }

    #[instrument(level = "trace", skip(self))]
    fn max_pitch(&self) -> f32 {
        100.
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_pitch(&self) -> f32 {
        0.
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_pitch(&self) -> Result<f32, Error> {
        self.connection.get_value(Request::GetPitch)
    }

    #[allow(clippy::cast_possible_truncation)]
    #[instrument(level = "debug", skip(self), err)]
    fn set_pitch(&mut self, pitch: f32) -> Result<(), Error> {
        self.connection
            .command(Request::SetPitch(ClientScope::Current, pitch as i8))
    }

    #[instrument(level = "trace", skip(self))]
    fn min_volume(&self) -> f32 {
        -100.
    }

    #[instrument(level = "trace", skip(self))]
    fn max_volume(&self) -> f32 {
        100.
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_volume(&self) -> f32 {
        100.
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_volume(&self) -> Result<f32, Error> {
        self.connection.get_value(Request::GetVolume)
    }

    #[allow(clippy::cast_possible_truncation)]
    #[instrument(level = "debug", skip(self), err)]
    fn set_volume(&mut self, volume: f32) -> Result<(), Error> {
        self.connection
            .command(Request::SetVolume(ClientScope::Current, volume as i8))
    }

    #[instrument(level = "trace", skip(self), err, ret)]
    fn is_speaking(&self) -> Result<bool, Error> {
        Ok(self.connection.state.lock().speaking)
    }

    #[instrument(level = "debug", skip(self), err)]
    fn voices(&self) -> Result<Vec<Voice>, Error> {
        let mut client = self.connection.lock_for_command();
        match self
            .connection
            .transact(&mut client, Request::ListSynthesisVoices)?
        {
            Response::VoicesListSent(voices) => Ok(voices
                .iter()
                .filter_map(|v| {
                    let language = LanguageTag::parse(v.language.clone()?).ok()?;
                    Some(Voice {
                        id: v.name.clone(),
                        name: v.name.clone(),
                        gender: None,
                        language,
                    })
                })
                .collect()),
            _ => Err(Error::NoneError),
        }
    }

    #[instrument(level = "debug", skip(self), err)]
    fn voice(&self) -> Result<Option<Voice>, Error> {
        unimplemented!()
    }

    #[instrument(level = "debug", skip(self), err)]
    fn set_voice(&mut self, voice: &Voice) -> Result<(), Error> {
        let mut client = self.connection.lock_for_command();
        match self
            .connection
            .transact(&mut client, Request::ListSynthesisVoices)?
        {
            Response::VoicesListSent(voices) => {
                if voices.iter().any(|v| v.name == voice.name) {
                    self.connection.transact(
                        &mut client,
                        Request::SetSynthesisVoice(ClientScope::Current, voice.name.clone()),
                    )?;
                    Ok(())
                } else {
                    Err(Error::OperationFailed)
                }
            }
            _ => Err(Error::NoneError),
        }
    }
}

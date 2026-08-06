use oxilangtag::LanguageTag;
use tracing::{info, instrument, warn};
use zbus::{
    blocking::{Connection, fdo::DBusProxy},
    names::BusName,
    proxy,
};

use crate::{Backend, Error, Features, SynthesizedAudio, UtteranceId, Voice};

const BUS_NAME: &str = "org.gnome.Orca1.Service";

#[proxy(
    interface = "org.gnome.Orca1.Service",
    default_service = "org.gnome.Orca1.Service",
    default_path = "/org/gnome/Orca1/Service",
    gen_async = false,
    blocking_name = "ServiceProxy"
)]
trait Service {
    fn speak_message(&self, message: &str) -> zbus::Result<bool>;

    fn get_version(&self) -> zbus::Result<String>;
}

#[proxy(
    interface = "org.gnome.Orca1.SpeechManager",
    default_service = "org.gnome.Orca1.Service",
    default_path = "/org/gnome/Orca1/Service/SpeechManager",
    gen_async = false,
    blocking_name = "SpeechManagerProxy"
)]
trait SpeechManager {
    fn interrupt_speech(&self, notify_user: bool) -> zbus::Result<bool>;

    fn get_voices_for_language(
        &self,
        language: &str,
        variant: &str,
    ) -> zbus::Result<Vec<(String, String, String)>>;

    // Orca never emits `PropertiesChanged`, so caching would go permanently stale.
    #[zbus(property(emits_changed_signal = "false"))]
    fn rate(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn set_rate(&self, value: u32) -> zbus::Result<()>;

    #[zbus(property(emits_changed_signal = "false"))]
    fn pitch(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    fn set_pitch(&self, value: f64) -> zbus::Result<()>;

    #[zbus(property(emits_changed_signal = "false"))]
    fn volume(&self) -> zbus::Result<f64>;

    #[zbus(property)]
    fn set_volume(&self, value: f64) -> zbus::Result<()>;

    #[zbus(property(emits_changed_signal = "false"))]
    fn current_voice(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn set_current_voice(&self, value: &str) -> zbus::Result<()>;
}

#[proxy(
    interface = "org.a11y.Status",
    default_service = "org.a11y.Bus",
    default_path = "/org/a11y/bus",
    gen_async = false,
    blocking_name = "A11yStatusProxy"
)]
trait A11yStatus {
    #[zbus(property(emits_changed_signal = "false"))]
    fn screen_reader_enabled(&self) -> zbus::Result<bool>;
}

/// Returns whether the desktop reports an active screen reader via the
/// accessibility bus (`org.a11y.Status.ScreenReaderEnabled`).
#[instrument(level = "debug", ret)]
pub(crate) fn a11y_screen_reader_enabled() -> bool {
    fn probe() -> zbus::Result<bool> {
        let connection = Connection::session()?;
        A11yStatusProxy::new(&connection)?.screen_reader_enabled()
    }
    match probe() {
        Ok(enabled) => enabled,
        Err(error) => {
            warn!(%error, "querying the accessibility bus failed");
            false
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Orca {
    service: ServiceProxy<'static>,
    speech: SpeechManagerProxy<'static>,
}

impl Orca {
    pub(crate) const NAME: &str = "Orca";

    /// Available only while Orca is running: its service is not D-Bus activatable.
    #[instrument(level = "debug", ret)]
    pub(crate) fn is_available() -> bool {
        fn probe() -> zbus::Result<bool> {
            let connection = Connection::session()?;
            let name = BusName::try_from(BUS_NAME)?;
            Ok(DBusProxy::new(&connection)?.name_has_owner(name)?)
        }
        match probe() {
            Ok(running) => running,
            Err(error) => {
                warn!(%error, "D-Bus probe for Orca failed");
                false
            }
        }
    }

    #[instrument(level = "info", err)]
    pub(crate) fn new() -> Result<Self, Error> {
        let connection = Connection::session()?;
        let service = ServiceProxy::new(&connection)?;
        let speech = SpeechManagerProxy::new(&connection)?;
        let version = service.get_version().map_err(|error| {
            warn!(%error, "Orca did not respond to GetVersion");
            Error::BackendUnavailable("Orca is not running")
        })?;
        info!(version, "Connected to Orca");
        Ok(Self { service, speech })
    }
}

impl Backend for Orca {
    #[instrument(level = "trace", skip(self))]
    fn supported_features(&self) -> Features {
        Features {
            stop: true,
            rate: true,
            pitch: true,
            volume: true,
            voice: true,
            get_voice: true,
            ..Default::default()
        }
    }

    #[instrument(level = "debug", skip(self), err)]
    fn speak(&mut self, text: &str, interrupt: bool) -> Result<Option<UtteranceId>, Error> {
        // `SpeakMessage` queues rather than interrupts, so interruption is a separate call.
        if interrupt {
            self.speech.interrupt_speech(false)?;
        }
        // The returned boolean is unconditionally true; muted or disabled speech
        // silently drops the message.
        self.service.speak_message(text)?;
        Ok(None)
    }

    #[instrument(level = "debug", skip(self, _text), err)]
    fn synthesize(&mut self, _text: &str) -> Result<SynthesizedAudio, Error> {
        unimplemented!()
    }

    #[instrument(level = "debug", skip(self), err)]
    fn stop(&mut self) -> Result<(), Error> {
        self.speech.interrupt_speech(false)?;
        Ok(())
    }

    #[instrument(level = "debug", skip(self), err)]
    fn pause(&mut self) -> Result<(), Error> {
        unimplemented!()
    }

    #[instrument(level = "debug", skip(self), err)]
    fn resume(&mut self) -> Result<(), Error> {
        unimplemented!()
    }

    #[instrument(level = "trace", skip(self), err, ret)]
    fn is_paused(&self) -> Result<bool, Error> {
        unimplemented!()
    }

    #[instrument(level = "trace", skip(self))]
    fn min_rate(&self) -> f32 {
        0.
    }

    #[instrument(level = "trace", skip(self))]
    fn max_rate(&self) -> f32 {
        100.
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_rate(&self) -> f32 {
        50.
    }

    #[allow(clippy::cast_precision_loss)]
    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_rate(&self) -> Result<f32, Error> {
        Ok(self.speech.rate()? as f32)
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    #[instrument(level = "debug", skip(self), err)]
    fn set_rate(&mut self, rate: f32) -> Result<(), Error> {
        self.speech.set_rate(rate as u32)?;
        Ok(())
    }

    #[instrument(level = "trace", skip(self))]
    fn min_pitch(&self) -> f32 {
        0.
    }

    #[instrument(level = "trace", skip(self))]
    fn max_pitch(&self) -> f32 {
        10.
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_pitch(&self) -> f32 {
        5.
    }

    #[allow(clippy::cast_possible_truncation)]
    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_pitch(&self) -> Result<f32, Error> {
        Ok(self.speech.pitch()? as f32)
    }

    #[instrument(level = "debug", skip(self), err)]
    fn set_pitch(&mut self, pitch: f32) -> Result<(), Error> {
        self.speech.set_pitch(f64::from(pitch))?;
        Ok(())
    }

    #[instrument(level = "trace", skip(self))]
    fn min_volume(&self) -> f32 {
        0.
    }

    #[instrument(level = "trace", skip(self))]
    fn max_volume(&self) -> f32 {
        10.
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_volume(&self) -> f32 {
        10.
    }

    #[allow(clippy::cast_possible_truncation)]
    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_volume(&self) -> Result<f32, Error> {
        Ok(self.speech.volume()? as f32)
    }

    #[instrument(level = "debug", skip(self), err)]
    fn set_volume(&mut self, volume: f32) -> Result<(), Error> {
        self.speech.set_volume(f64::from(volume))?;
        Ok(())
    }

    #[instrument(level = "trace", skip(self), err, ret)]
    fn is_speaking(&self) -> Result<bool, Error> {
        unimplemented!()
    }

    #[instrument(level = "debug", skip(self), err)]
    fn voices(&self) -> Result<Vec<Voice>, Error> {
        // An empty language resolves to the current locale; Orca offers no way to
        // enumerate every voice together with a language tag.
        let families = self.speech.get_voices_for_language("", "")?;
        Ok(families
            .into_iter()
            .filter_map(|(name, language, _variant)| {
                LanguageTag::parse(language).ok().map(|language| Voice {
                    id: name.clone(),
                    name,
                    gender: None,
                    language,
                })
            })
            .collect())
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn voice(&self) -> Result<Option<Voice>, Error> {
        let name = self.speech.current_voice()?;
        if name.is_empty() {
            return Ok(None);
        }
        match self.voices()?.into_iter().find(|voice| voice.name == name) {
            Some(voice) => Ok(Some(voice)),
            None => Err(Error::VoiceNotFound(name)),
        }
    }

    #[instrument(level = "debug", skip(self), err)]
    fn set_voice(&mut self, voice: &Voice) -> Result<(), Error> {
        // Orca validates the name against its available voices and rejects the
        // write with a method error rather than a typed one.
        self.speech
            .set_current_voice(&voice.name)
            .map_err(|error| match error {
                zbus::Error::MethodError(..) => Error::VoiceNotFound(voice.name.clone()),
                error => error.into(),
            })
    }
}

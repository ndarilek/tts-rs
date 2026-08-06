#[cfg(all(windows, feature = "tolk"))]
use std::sync::Arc;

use tolk::Tolk as TolkPtr;
use tracing::instrument;

use crate::{Backend, Error, Features, SynthesizedAudio, UtteranceId, Voice};

#[derive(Clone, Debug)]
pub(crate) struct Tolk(Arc<TolkPtr>);

impl Tolk {
    pub(crate) const NAME: &str = "Tolk";

    /// Available only while a screen reader is running to receive the speech.
    #[instrument(level = "debug", ret)]
    pub(crate) fn is_available() -> bool {
        TolkPtr::new().detect_screen_reader().is_some()
    }

    #[instrument(level = "info")]
    pub(crate) fn new() -> Option<Self> {
        let tolk = TolkPtr::new();
        if tolk.detect_screen_reader().is_some() {
            Some(Tolk(tolk))
        } else {
            None
        }
    }
}

impl Backend for Tolk {
    #[instrument(level = "trace", skip(self))]
    fn supported_features(&self) -> Features {
        Features {
            stop: true,
            ..Default::default()
        }
    }

    #[instrument(level = "debug", skip(self), err)]
    fn speak(&mut self, text: &str, interrupt: bool) -> Result<Option<UtteranceId>, Error> {
        self.0.speak(text, interrupt);
        Ok(None)
    }

    #[instrument(level = "debug", skip(self, _text), err)]
    fn synthesize(&mut self, _text: &str) -> Result<SynthesizedAudio, Error> {
        unimplemented!()
    }

    #[instrument(level = "debug", skip(self), err)]
    fn stop(&mut self) -> Result<(), Error> {
        self.0.silence();
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
        unimplemented!()
    }

    #[instrument(level = "trace", skip(self))]
    fn max_rate(&self) -> f32 {
        unimplemented!()
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_rate(&self) -> f32 {
        unimplemented!()
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_rate(&self) -> Result<f32, Error> {
        unimplemented!();
    }

    #[instrument(level = "debug", skip(self, _rate), err)]
    fn set_rate(&mut self, _rate: f32) -> Result<(), Error> {
        unimplemented!();
    }

    #[instrument(level = "trace", skip(self))]
    fn min_pitch(&self) -> f32 {
        unimplemented!()
    }

    #[instrument(level = "trace", skip(self))]
    fn max_pitch(&self) -> f32 {
        unimplemented!()
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_pitch(&self) -> f32 {
        unimplemented!()
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_pitch(&self) -> Result<f32, Error> {
        unimplemented!();
    }

    #[instrument(level = "debug", skip(self, _pitch), err)]
    fn set_pitch(&mut self, _pitch: f32) -> Result<(), Error> {
        unimplemented!();
    }

    #[instrument(level = "trace", skip(self))]
    fn min_volume(&self) -> f32 {
        unimplemented!()
    }

    #[instrument(level = "trace", skip(self))]
    fn max_volume(&self) -> f32 {
        unimplemented!()
    }

    #[instrument(level = "trace", skip(self))]
    fn normal_volume(&self) -> f32 {
        unimplemented!()
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn get_volume(&self) -> Result<f32, Error> {
        unimplemented!();
    }

    #[instrument(level = "debug", skip(self, _volume), err)]
    fn set_volume(&mut self, _volume: f32) -> Result<(), Error> {
        unimplemented!();
    }

    #[instrument(level = "trace", skip(self), err, ret)]
    fn is_speaking(&self) -> Result<bool, Error> {
        unimplemented!()
    }

    #[instrument(level = "debug", skip(self), err, ret)]
    fn voice(&self) -> Result<Option<Voice>, Error> {
        unimplemented!()
    }

    #[instrument(level = "debug", skip(self), err)]
    fn voices(&self) -> Result<Vec<Voice>, Error> {
        unimplemented!()
    }

    #[instrument(level = "debug", skip(self, _voice), err)]
    fn set_voice(&mut self, _voice: &Voice) -> Result<(), Error> {
        unimplemented!()
    }
}

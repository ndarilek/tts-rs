
use objc::runtime::*;
use objc::*;
use core_foundation::array::CFArray;
use core_foundation::string::CFString;

#[derive(Copy,Clone)]
pub struct AVSpeechSynthesisVoice(*const Object);

impl AVSpeechSynthesisVoice {
    pub fn new(identifier: String) -> Self {
        let i: CFString = CFString::from(identifier.as_str());
        let voice: *const Object = unsafe{msg_send![class!(AVSpeechSynthesisVoice).metaclass(), voiceWithIdentifier: i]};
        AVSpeechSynthesisVoice{0: voice}
    }

    pub fn default() -> Self {
        AVSpeechSynthesisVoice::list()[0]
    }

    pub fn list() -> Vec<Self> {
        let voices: CFArray = unsafe{msg_send![class!(AVSpeechSynthesisVoice), speechVoices]};
        voices.iter().map(|v| {
            AVSpeechSynthesisVoice{0: *v as *mut Object}
        }).collect()
    }

    pub fn name(self) -> String {
        let name: CFString = unsafe{msg_send![self.0, name]};
        name.to_string()
    }

    pub fn identifier(self) -> String {
        let identifier: CFString = unsafe{msg_send![self.0, identifier]};
        identifier.to_string()
    }
}

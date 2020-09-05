
use objc::runtime::*;
use objc::*;
use core_foundation::array::CFArray;
use cocoa_foundation::foundation::NSString;
use cocoa_foundation::base::{nil,id};
use core_foundation::string::CFString;

#[derive(Copy,Clone)]
pub struct AVSpeechSynthesisVoice(*const Object);

impl AVSpeechSynthesisVoice {
    pub fn new(identifier: &str) -> Self {
        let voice: *const Object;
        unsafe{
            let i: id = NSString::alloc(nil).init_str(identifier);
            voice = msg_send![class!(AVSpeechSynthesisVoice), voiceWithIdentifier:i];
        };
        AVSpeechSynthesisVoice{0:voice}
    }

    pub fn default() -> Self {
        AVSpeechSynthesisVoice::list()[0]
    }

    pub fn list() -> Vec<Self> {
        let voices: CFArray = unsafe{msg_send![class!(AVSpeechSynthesisVoice), speechVoices]};
        voices.iter().map(|v| {
            AVSpeechSynthesisVoice{0: *v as *const Object}
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

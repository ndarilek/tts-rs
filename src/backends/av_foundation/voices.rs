use cocoa_foundation::base::{id, nil};
use cocoa_foundation::foundation::NSString;
use core_foundation::array::CFArray;
use core_foundation::string::CFString;
use objc::runtime::*;
use objc::*;

use crate::backends::AvFoundation;
use crate::voices;
use crate::voices::Gender;

#[derive(Copy, Clone, Debug)]
pub(crate) struct AVSpeechSynthesisVoice(*const Object);

impl AVSpeechSynthesisVoice {
    pub fn new() -> Self {
        let voice: *const Object;
        unsafe {
            voice = msg_send![class!(AVSpeechSynthesisVoice), new];
        };
        AVSpeechSynthesisVoice { 0: voice }
    }
}

impl voices::Backend for AVSpeechSynthesisVoice {
    type Backend = AvFoundation;

    fn from_id(id: String) -> Self {
        unimplemented!()
    }

    fn from_language(lang: voices::LanguageIdentifier) -> Self {
        unimplemented!()
    }

    fn list() -> Vec<Self> {
        let voices: CFArray = unsafe { msg_send![class!(AVSpeechSynthesisVoice), speechVoices] };
        voices
            .iter()
            .map(|v| AVSpeechSynthesisVoice {
                0: *v as *const Object,
            })
            .collect()
    }

    fn name(self) -> String {
        let name: CFString = unsafe { msg_send![self.0, name] };
        name.to_string()
    }

    fn gender(self) -> Gender {
        let gender: i64 = unsafe { msg_send![self.0, gender] };
        match gender {
            1 => Gender::Male,
            2 => Gender::Female,
            _ => Gender::Other,
        }
    }

    fn id(self) -> String {
        let identifier: CFString = unsafe { msg_send![self.0, identifier] };
        identifier.to_string()
    }

    fn language(self) -> voices::LanguageIdentifier {
        let lang: CFString = unsafe { msg_send![self.0, language] };
        lang.to_string().parse().unwrap()
    }
}

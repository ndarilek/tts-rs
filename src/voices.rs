pub use unic_langid::LanguageIdentifier;

pub enum Gender {
    Other,
    Male,
    Female,
}

pub trait Backend: Sized {
    type Backend: crate::Backend;
    fn from_id(id: String) -> Self;
    fn from_language(lang: LanguageIdentifier) -> Self;
    fn list() -> Vec<Self>;
    fn name(self) -> String;
    fn gender(self) -> Gender;
    fn id(self) -> String;
    fn language(self) -> LanguageIdentifier;
}

pub struct Voice<T: Backend + Sized>(Box<T>);

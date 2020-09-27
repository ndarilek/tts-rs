
pub use unic_langid::LanguageIdentifier;

pub enum Gender {
    Other,
    Male,
    Female,
}

pub trait Backend: Sized {
    type Backend: crate::Backend;
    fn list() -> Vec<Self>;
    fn name(self) -> String;
    fn gender(self) -> Gender;
    fn id(self) -> String;
    fn language(self) -> LanguageIdentifier;
}

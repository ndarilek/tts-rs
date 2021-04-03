#![allow(clippy::wildcard_imports)]
use seed::{prelude::*, *};

use tts::Tts;

#[derive(Clone)]
struct Model {
    text: String,
    tts: Tts,
}

#[derive(Clone)]
enum Msg {
    TextChanged(String),
    RateChanged(String),
    PitchChanged(String),
    VolumeChanged(String),
    Speak,
}

fn init(_: Url, _: &mut impl Orders<Msg>) -> Model {
    let tts = Tts::default().unwrap();
    Model {
        text: Default::default(),
        tts,
    }
}

fn update(msg: Msg, model: &mut Model, _: &mut impl Orders<Msg>) {
    use Msg::*;
    match msg {
        TextChanged(text) => model.text = text,
        RateChanged(rate) => {
            let rate = rate.parse::<f32>().unwrap();
            model.tts.set_rate(rate).unwrap();
        }
        PitchChanged(pitch) => {
            let pitch = pitch.parse::<f32>().unwrap();
            model.tts.set_pitch(pitch).unwrap();
        }
        VolumeChanged(volume) => {
            let volume = volume.parse::<f32>().unwrap();
            model.tts.set_volume(volume).unwrap();
        }
        Speak => {
            model.tts.speak(&model.text, false).unwrap();
        }
    }
}

fn view(model: &Model) -> Node<Msg> {
    form![
        div![label![
            "Text to speak",
            input![
                attrs! {
                    At::Value => model.text,
                    At::AutoFocus => AtValue::None,
                },
                input_ev(Ev::Input, Msg::TextChanged)
            ],
        ],],
        div![label![
            "Rate",
            input![
                attrs! {
                    At::Type => "number",
                    At::Value => model.tts.get_rate().unwrap(),
                    At::Min => model.tts.min_rate(),
                    At::Max => model.tts.max_rate()
                },
                input_ev(Ev::Input, Msg::RateChanged)
            ],
        ],],
        div![label![
            "Pitch",
            input![
                attrs! {
                    At::Type => "number",
                    At::Value => model.tts.get_pitch().unwrap(),
                    At::Min => model.tts.min_pitch(),
                    At::Max => model.tts.max_pitch()
                },
                input_ev(Ev::Input, Msg::PitchChanged)
            ],
        ],],
        div![label![
            "Volume",
            input![
                attrs! {
                    At::Type => "number",
                    At::Value => model.tts.get_volume().unwrap(),
                    At::Min => model.tts.min_volume(),
                    At::Max => model.tts.max_volume()
                },
                input_ev(Ev::Input, Msg::VolumeChanged)
            ],
        ],],
        button![
            "Speak",
            ev(Ev::Click, |e| {
                e.prevent_default();
                Msg::Speak
            }),
        ],
    ]
}

fn main() {
    App::start("app", init, update, view);
}

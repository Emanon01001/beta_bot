use poise::ChoiceParameter;

#[derive(Copy, Clone, Debug, ChoiceParameter)]
pub enum RepeatMode {
    #[name = "Off"]
    Off,
    #[name = "Track"]
    Track,
    #[name = "Queue"]
    Queue,
}

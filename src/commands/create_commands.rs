use crate::commands;

pub fn create_commands()
-> Vec<poise::Command<crate::Data, Box<dyn std::error::Error + Send + Sync + 'static>>> {
    let commands: Vec<
        poise::Command<crate::Data, Box<dyn std::error::Error + Send + Sync + 'static>>,
    > = vec![
        commands::music::play::play(),
        commands::music::join::join(),
        commands::music::leave::leave(),
        commands::music::insert::insert(),
        commands::music::queue::queue(),
        commands::music::stop::stop(),
        commands::music::skip::skip(),
        commands::music::pause::pause(),
        commands::music::resume::resume(),
        commands::music::repeat::repeat(),
        commands::music::shuffle::shuffle(),
        commands::music::search::search(),
        commands::test::button_test(),
        commands::test::exec(),
    ];
    commands
}

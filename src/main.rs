mod cli;
mod state;
mod ui;

use cli::Args;
use state::AppState;
use ui::UI;

async fn run_app<'a>(ui: &mut ui::UI<'a>) -> ! {
    loop {
        match ui.update().await {
            Ok(true) => (),
            Ok(false) => exit_app(None),
            Err(error) => exit_app(Some(error.to_string())),
        }
    }
}

fn exit_app(error: Option<String>) -> ! {
    if let Some(err_msg) = error {
        println!("An error occurred: {}", err_msg);
        std::process::exit(1);
    } else {
        std::process::exit(0);
    }
}

#[tokio::main]
async fn main() -> Result<(), kube::Error> {

    let Args { namespace: namespace_opt } = Args::collect();

    let mut app_state = AppState::new(namespace_opt).await?;
    let mut ui = UI::new(&mut app_state);

    run_app(&mut ui).await;

    Ok(())
}

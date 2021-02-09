use druid::{AppLauncher, Widget, WindowDesc};
use druid::widget::Label;
use crate::ShutdownMessage;
use tokio::sync::mpsc::UnboundedSender;

pub fn run(shutdown_tx: UnboundedSender<ShutdownMessage>) {
    //let restart_tx = shutdown_tx.clone(); //TODO: pass to some UI button somewhere
    AppLauncher::with_window(WindowDesc::new(build_ui)).launch(()).expect("could not instantiate window");
    match shutdown_tx.send(ShutdownMessage::Shutdown) {
        Ok(()) => println!("shutdown triggered by UI close"),
        Err(e) => panic!("Error triggering shutdown: {}", e)
    };
}

fn build_ui() -> impl Widget<()> {
    Label::new("Hello world")
}

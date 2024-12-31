pub mod prelude;
pub mod spec;
pub mod scope;
pub mod events;

use ctrlc;

pub async fn serve_asgi<S, A, T>(server: S, application: A, state: T) -> std::result::Result<(), Box<dyn std::error::Error>> 
where
    T: spec::State,
    A: spec::ASGIApplication<T>,
    S: spec::ASGIServer<T, A>,
{
    // TODO: test the ctrlc handler
    let token = server.serve(application, state).await?;

     ctrlc::set_handler(move || token.send(())
        .expect("Could not send signal on channel."))
        .expect("Error setting Ctrl-C handler");

    Ok(())
}
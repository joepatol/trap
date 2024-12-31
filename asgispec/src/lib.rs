pub mod prelude;
pub mod spec;
pub mod scope;
pub mod events;

pub async fn serve_asgi<S, A, T>(server: S, application: A, state: T) -> std::result::Result<(), Box<dyn std::error::Error>> 
where
    T: spec::State,
    S: spec::ASGIServer<T>,
    A: spec::ASGIApplication<T>,
{
    // TODO: actually use cancel token here. Should this function be async? Current idea is that server starts the runtime. So the spec is runtime independent.
    let _ = server.serve(application, state).await?;
    Ok(())
}
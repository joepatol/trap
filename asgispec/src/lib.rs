pub mod prelude;
pub mod spec;
pub mod scope;
pub mod events;

pub async fn serve_asgi<S, A>(server: S, application: A, state: A::State) -> S::Output
where
    A: spec::ASGIApplication,
    S: spec::ASGIServer<A>,
{
    server.run(application, state).await
}
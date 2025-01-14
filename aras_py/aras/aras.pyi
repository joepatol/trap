from .aras_types import ASGIApplication

def serve(
    application: ASGIApplication,
    addr: list[int] = [127, 0, 0, 1],
    port: int = 8080,
    keep_alive: bool = True,
    max_concurrency: int | None = None,
    max_size_kb: int = 1_000_000,
) -> None: ...

from dataclasses import dataclass


@dataclass
class ServerConfig:
    host: str
    port: int
    keep_alive: bool
    max_concurrency: int | None
    max_size_kb: int
    request_timeout: int
    rate_limit: tuple[int, int]
    buffer_size: int
    backpressure_timeout: int
    backpressure_size: int
    max_ws_frame_size: int
    request_ids: bool
    auto_date_header: bool
    sensitive_headers: list[str] | None

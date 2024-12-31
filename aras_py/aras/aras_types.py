from typing import Any, Awaitable, Callable, TypeAlias, Protocol, Literal, MutableMapping

Send: TypeAlias = Callable[[MutableMapping[str, Any]], Awaitable[None]]
Receive: TypeAlias = Callable[[], Awaitable[MutableMapping[str, Any]]]
Scope: TypeAlias = MutableMapping[str, Any]

LogLevel = Literal["DEBUG", "INFO", "WARN", "TRACE", "OFF", "ERROR"]

class ASGIApplication(Protocol):
    async def __call__(self, scope: Scope, receive: Receive, send: Send) -> None: ...

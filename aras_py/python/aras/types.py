from typing import (Any, Awaitable, Callable, MutableMapping, Protocol,
                    TypeAlias)

Send: TypeAlias = Callable[[MutableMapping[str, Any]], Awaitable[None]]
Receive: TypeAlias = Callable[[], Awaitable[MutableMapping[str, Any]]]
Scope: TypeAlias = MutableMapping[str, Any]


class ASGIApplication(Protocol):
    async def __call__(self, scope: Scope, receive: Receive, send: Send) -> None: ...


class CancelToken(Protocol):
    def stop(self) -> None: ...

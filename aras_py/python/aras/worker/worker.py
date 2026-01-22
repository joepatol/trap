import importlib
import msgpack
import asyncio
from asyncio import StreamReader, StreamWriter
import argparse
from typing import TypedDict, MutableMapping, Any
from aras.types import ASGIApplication


async def main() -> None:
    args = parse_args()
    
    app = import_asgi_app(args["app"])
    worker = Worker(app)

    server = await asyncio.start_unix_server(worker.serve_asgi_client, args["socket"])
    
    async with server:
        await server.serve_forever()


class ParsedArgs(TypedDict):
    socket: str
    app: str


def parse_args() -> ParsedArgs:
    parser = argparse.ArgumentParser()

    for arg in ["--socket", "--app"]:
        parser.add_argument(arg, required=True)

    args = parser.parse_args()

    return {
        "socket": args.socket,
        "app": args.app,
    }


def import_asgi_app(import_str: str) -> None:
    import_path, app_name = import_str.split(":")
    module = importlib.import_module(import_path)
    return getattr(module, app_name)


async def read_next_message(reader: StreamReader) -> MutableMapping[str, Any]:
    length = int.from_bytes(await reader.readexactly(4), byteorder="big")

    data = await reader.readexactly(length)
    message = msgpack.unpackb(data, raw=False)

    return message


class _Send:
    def __init__(self, writer: StreamWriter) -> None:
        self._writer = writer
    
    async def __call__(self, message: MutableMapping[str, Any]) -> None:
        data = msgpack.packb(message)
        length = int.to_bytes(len(data), length=4, byteorder="big")

        self._writer.write(length + data)
        await self._writer.drain()


class _Receive:
    def __init__(self, reader: StreamReader) -> None:
        self._reader = reader
        self._disconnect_msg = None
    
    async def __call__(self) -> MutableMapping[str, Any]:
        if self._disconnect_msg is not None:
            return self._disconnect_msg
        
        message = await read_next_message(self._reader)

        if message.get("type") in ["http.disconnect", "websocket.disconnect"]:
            self._disconnect_msg = message

        return message


class Worker:
    def __init__(self, app: ASGIApplication) -> None:
        self.app = app

    async def serve_asgi_client(self, reader: StreamReader, writer: StreamWriter) -> None:
        scope = await read_next_message(reader)

        send = self.build_send(writer)
        receive = self.build_receive(reader)

        await self.app(scope, receive, send)
    
    def build_send(self, writer: StreamWriter) -> _Send:
        return _Send(writer)
    
    def build_receive(self, reader: StreamReader) -> _Receive:
        return _Receive(reader)

    
if __name__ == "__main__":
    asyncio.run(main())

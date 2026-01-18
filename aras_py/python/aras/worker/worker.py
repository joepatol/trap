import importlib
import logging
import msgpack
import asyncio
from asyncio import StreamReader, StreamWriter
import argparse
from typing import TypedDict, MutableMapping, Any
from aras.types import Send, Receive, ASGIApplication

"""
Message format over the unix socket:

worker_id|length|data\n
"""

logger = logging.getLogger("aras.worker")

ARGS = [
    "--id",
    "--socket",
    "--app",
]

async def main() -> None:
    args = parse_args()

    worker_id = args["id"]
    
    app = import_asgi_app(args["app"])
    worker = Worker(worker_id, app)

    server = await asyncio.start_unix_server(worker.serve_asgi_client, args["socket"])
    logger.info(f"Started worker with id {worker_id}")
    
    async with server:
        await server.serve_forever()
        
    logger.info(f"Worker {worker_id} exited")


class ParsedArgs(TypedDict):
    id: str
    socket: str
    app: str


def parse_args() -> ParsedArgs:
    parser = argparse.ArgumentParser()

    for arg in ARGS:
        parser.add_argument(arg, required=True)

    args = parser.parse_args()

    return {
        "id": args.id,
        "socket": args.socket,
        "app": args.app,
    }


def import_asgi_app(import_str: str) -> ASGIApplication:
    module_path, app_name = import_str.rsplit(":", 1)
    module = importlib.import_module(module_path)
    return getattr(module, app_name)


class Worker:
    def __init__(self, worker_id: str, app: ASGIApplication) -> None:
        self.worker_id = worker_id
        self.app = app

    async def serve_asgi_client(self, reader: StreamReader, writer: StreamWriter) -> None:
        scope = await self.read_next_message(reader)

        send = self.build_send(writer)
        receive = self.build_receive(reader)

        await self.app(scope, receive, send)

        writer.write(f"{self.worker_id}|4|done\n".encode())
        await writer.drain()
    
    async def read_next_message(self, reader: StreamReader) -> MutableMapping[str, Any]:
        worker_id_data = await reader.readuntil(b"|")
        recv_worker_id = int(worker_id_data.rstrip(b"|").decode())

        if self.worker_id != recv_worker_id:
            raise RuntimeError(f"Worker {self.worker_id} received message meant for worker {recv_worker_id}")

        length_data = await reader.readuntil(b"|")
        length = int(length_data.rstrip(b"|").decode())

        data = await reader.readexactly(length)

        await reader.readuntil(b"\n")

        message = msgpack.unpackb(data, raw=False)
        return message
    
    def build_send(self, writer: StreamWriter) -> Send:
        async def send(asgi_message: MutableMapping[str, Any]) -> None:
            data = msgpack.packb(asgi_message, use_bin_type=True)
            message = f"{self.worker_id}|{len(data)}|".encode() + data + b"\n"

            writer.write(message)
            await writer.drain()
        
        return send
    
    def build_receive(self, reader: StreamReader) -> Receive:
        async def receive() -> MutableMapping[str, Any]:
            return await self.read_next_message(reader)
        
        return receive


if __name__ == "__main__":
    asyncio.run(main())

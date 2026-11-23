"""Loopback TCP relay with independent ordered delays for each stream direction."""
import asyncio
import concurrent.futures
import json
import threading
import time


class LatencyRelay:
    def __init__(self, target_port, uplink_ms, downlink_ms, logfile=None):
        self.target_port = target_port
        self.delays = uplink_ms, downlink_ms
        self.logfile = logfile
        self.connection_sequence = 0
        self.forwarded = 0
        self.ready = concurrent.futures.Future()
        self.thread = threading.Thread(target=self._run, name="parity-latency-relay")

    def set_delays(self, uplink_ms, downlink_ms):
        self.loop.call_soon_threadsafe(setattr, self, "delays", (uplink_ms, downlink_ms))

    def __enter__(self):
        self.thread.start()
        self.port = self.ready.result(timeout=10)
        return self

    def __exit__(self, *_):
        self.loop.call_soon_threadsafe(self.stop.set)
        self.thread.join(timeout=10)
        if self.thread.is_alive():
            raise RuntimeError("Latency relay did not stop")

    def _run(self):
        try:
            if self.logfile is None:
                self.log = None
                asyncio.run(self._serve())
            else:
                with self.logfile.open("w", encoding="utf-8") as self.log:
                    self.log.write('{"schema":1,"event":"manifest","scope":"tcp-chunk-relay-clock"}\n')
                    asyncio.run(self._serve())
                    self.log.write(json.dumps({"schema": 1, "event": "end", "forwarded": self.forwarded}) + "\n")
        except BaseException as exc:
            if not self.ready.done():
                self.ready.set_exception(exc)
            else:
                raise

    async def _serve(self):
        self.loop = asyncio.get_running_loop()
        self.stop = asyncio.Event()
        server = await asyncio.start_server(self._relay, "127.0.0.1", 0)
        async with server:
            self.ready.set_result(server.sockets[0].getsockname()[1])
            await self.stop.wait()

    async def _relay(self, client_reader, client_writer):
        connection = self.connection_sequence
        self.connection_sequence += 1
        server_writer = None
        try:
            server_reader, server_writer = await asyncio.open_connection("127.0.0.1", self.target_port)
            async with asyncio.TaskGroup() as group:
                group.create_task(self._direction(client_reader, server_writer, connection, 0))
                group.create_task(self._direction(server_reader, client_writer, connection, 1))
        except* ConnectionError:
            # Client and Paper disconnects also end the two forwarding tasks.
            pass
        finally:
            client_writer.close()
            if server_writer is not None:
                server_writer.close()

    async def _direction(self, reader, writer, connection, direction):
        pending = asyncio.Queue(maxsize=64)

        async def read():
            while data := await reader.read(32768):
                received = time.monotonic_ns()
                await pending.put((received, received + self.delays[direction] * 1_000_000, data))
            await pending.put((0, 0, b""))

        async def write():
            while True:
                received, release, data = await pending.get()
                if not data:
                    if writer.can_write_eof():
                        writer.write_eof()
                    return
                while (remaining := release - time.monotonic_ns()) > 0:
                    await asyncio.sleep(remaining / 1_000_000_000)
                forwarded = time.monotonic_ns()
                writer.write(data)
                if self.log is not None:
                    self.log.write(json.dumps({"schema": 1, "event": "forward", "sequence": self.forwarded,
                        "connection": connection, "direction": ("uplink", "downlink")[direction],
                        "received_ns": received, "deadline_ns": release, "forwarded_ns": forwarded,
                        "bytes": len(data)}) + "\n")
                self.forwarded += 1
                await writer.drain()

        async with asyncio.TaskGroup() as group:
            group.create_task(read())
            group.create_task(write())

#!/usr/bin/env python3
"""Loopback HTTPS CONNECT proxy that tunnels only allowlisted host:port targets.

Seatbelt cannot filter by hostname, so E1 agent sandboxes allow outbound traffic only to this proxy on
localhost. The proxy permits CONNECT to the model endpoint(s) and refuses everything else, including
plain HTTP. Every decision is recorded and, when run as a script, appended as JSON lines to --log.
"""

from __future__ import annotations

import argparse
import json
import select
import socket
import sys
import threading
import time
from pathlib import Path


class EgressProxy:
    def __init__(self, allowed: set[str], host: str = "127.0.0.1", port: int = 0, log_path: Path | None = None):
        self.allowed = {a.lower() for a in allowed}
        self.decisions: list[dict] = []
        self._log_path = log_path
        self._lock = threading.Lock()
        self._server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self._server.bind((host, port))
        self._server.listen(64)
        self.port = self._server.getsockname()[1]
        self._stopped = threading.Event()

    def start(self) -> None:
        threading.Thread(target=self._accept_loop, daemon=True).start()

    def stop(self) -> None:
        self._stopped.set()
        try:
            self._server.close()
        except OSError:
            pass

    def _record(self, target: str, allowed: bool, method: str) -> None:
        entry = {"ts": time.time(), "method": method, "target": target, "allowed": allowed}
        with self._lock:
            self.decisions.append(entry)
            if self._log_path:
                with self._log_path.open("a") as handle:
                    handle.write(json.dumps(entry) + "\n")

    def _accept_loop(self) -> None:
        while not self._stopped.is_set():
            try:
                client, _ = self._server.accept()
            except OSError:
                return
            threading.Thread(target=self._handle, args=(client,), daemon=True).start()

    def _handle(self, client: socket.socket) -> None:
        client.settimeout(30)
        try:
            head = b""
            while b"\r\n\r\n" not in head and len(head) < 65536:
                chunk = client.recv(4096)
                if not chunk:
                    return
                head += chunk
            request_line = head.split(b"\r\n", 1)[0].decode("latin-1")
            parts = request_line.split()
            method = parts[0].upper() if parts else ""
            target = parts[1].lower() if len(parts) > 1 else ""
            if method != "CONNECT" or target not in self.allowed:
                self._record(target, False, method)
                client.sendall(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                return
            host, _, port = target.rpartition(":")
            upstream = socket.create_connection((host, int(port)), timeout=30)
            self._record(target, True, method)
            client.sendall(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            self._pipe(client, upstream)
        except OSError:
            return
        finally:
            client.close()

    @staticmethod
    def _pipe(a: socket.socket, b: socket.socket) -> None:
        a.settimeout(None)
        b.settimeout(None)
        try:
            while True:
                readable, _, _ = select.select([a, b], [], [], 600)
                if not readable:
                    return
                for source in readable:
                    data = source.recv(65536)
                    if not data:
                        return
                    (b if source is a else a).sendall(data)
        finally:
            b.close()


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--allow", action="append", required=True, help="host:port to permit (repeatable)")
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--log", type=Path, required=True)
    parser.add_argument("--port-file", type=Path, help="write the bound port here once listening")
    args = parser.parse_args(argv)
    proxy = EgressProxy(set(args.allow), port=args.port, log_path=args.log)
    proxy.start()
    if args.port_file:
        args.port_file.write_text(str(proxy.port))
    print(f"egress proxy listening on 127.0.0.1:{proxy.port} allow={sorted(proxy.allowed)}", flush=True)
    try:
        while True:
            time.sleep(3600)
    except KeyboardInterrupt:
        proxy.stop()
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))

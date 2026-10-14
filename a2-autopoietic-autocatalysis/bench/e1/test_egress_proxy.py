"""Tests for bench/e1/egress_proxy.py. Run: python3 -m unittest bench/e1/test_egress_proxy.py"""

import socket
import sys
import threading
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import egress_proxy  # noqa: E402


def start_echo_server() -> tuple[socket.socket, int]:
    server = socket.socket()
    server.bind(("127.0.0.1", 0))
    server.listen(4)

    def serve() -> None:
        while True:
            try:
                conn, _ = server.accept()
            except OSError:
                return
            data = conn.recv(1024)
            conn.sendall(b"echo:" + data)
            conn.close()

    threading.Thread(target=serve, daemon=True).start()
    return server, server.getsockname()[1]


def request(proxy_port: int, payload: bytes) -> bytes:
    with socket.create_connection(("127.0.0.1", proxy_port), timeout=5) as conn:
        conn.sendall(payload)
        chunks = []
        while True:
            try:
                chunk = conn.recv(4096)
            except socket.timeout:
                break
            if not chunk:
                break
            chunks.append(chunk)
            if b"echo:" in b"".join(chunks) or b"403" in b"".join(chunks):
                break
        return b"".join(chunks)


class EgressProxyTests(unittest.TestCase):
    def setUp(self) -> None:
        self.echo, self.echo_port = start_echo_server()
        self.proxy = egress_proxy.EgressProxy({f"127.0.0.1:{self.echo_port}"})
        self.proxy.start()
        self.addCleanup(self.proxy.stop)
        self.addCleanup(self.echo.close)

    def test_allowed_connect_is_tunnelled(self) -> None:
        with socket.create_connection(("127.0.0.1", self.proxy.port), timeout=5) as conn:
            conn.sendall(f"CONNECT 127.0.0.1:{self.echo_port} HTTP/1.1\r\nHost: x\r\n\r\n".encode())
            head = conn.recv(1024)
            self.assertIn(b"200", head.split(b"\r\n")[0])
            conn.sendall(b"ping")
            self.assertEqual(conn.recv(1024), b"echo:ping")

    def test_disallowed_connect_is_refused_and_logged(self) -> None:
        reply = request(self.proxy.port, b"CONNECT github.com:443 HTTP/1.1\r\nHost: github.com\r\n\r\n")
        self.assertIn(b"403", reply.split(b"\r\n")[0])
        self.assertIn(("github.com:443", False), [(d["target"], d["allowed"]) for d in self.proxy.decisions])

    def test_plain_http_is_refused(self) -> None:
        reply = request(self.proxy.port, f"GET http://127.0.0.1:{self.echo_port}/ HTTP/1.1\r\n\r\n".encode())
        self.assertIn(b"403", reply.split(b"\r\n")[0])

    def test_port_must_match(self) -> None:
        reply = request(self.proxy.port, b"CONNECT 127.0.0.1:1 HTTP/1.1\r\n\r\n")
        self.assertIn(b"403", reply.split(b"\r\n")[0])


if __name__ == "__main__":
    unittest.main()

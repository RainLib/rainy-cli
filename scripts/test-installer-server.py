#!/usr/bin/env python3
import http.server
import pathlib
import sys
import threading


root = pathlib.Path(sys.argv[1]).resolve()
port_file = pathlib.Path(sys.argv[2]).resolve()
failures_remaining = int(sys.argv[3]) if len(sys.argv) > 3 else 0
failure_lock = threading.Lock()


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=root, **kwargs)

    def log_message(self, format, *args):
        pass

    def do_GET(self):
        global failures_remaining
        with failure_lock:
            if failures_remaining > 0:
                failures_remaining -= 1
                self.send_response(http.HTTPStatus.SERVICE_UNAVAILABLE)
                self.end_headers()
                return
        super().do_GET()


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
port_file.write_text(str(server.server_port), encoding="ascii")
server.serve_forever()

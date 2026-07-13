#!/usr/bin/env python3
import http.server
import pathlib
import sys


root = pathlib.Path(sys.argv[1]).resolve()
port_file = pathlib.Path(sys.argv[2]).resolve()


class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=root, **kwargs)

    def log_message(self, format, *args):
        pass


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
port_file.write_text(str(server.server_port), encoding="ascii")
server.serve_forever()

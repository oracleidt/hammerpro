import json
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

port = int(sys.argv[1])
zip_path = sys.argv[2]
tag = sys.argv[3]
delay = float(sys.argv[4]) if len(sys.argv) > 4 else 0.0
with open(zip_path, "rb") as f:
    payload = f.read()
version = tag.lstrip("v")


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        sys.stderr.write("fake-server: " + (fmt % args) + "\n")

    def do_GET(self):
        base = f"http://127.0.0.1:{port}"
        if self.path == "/latest":
            body = json.dumps(
                {
                    "tag_name": tag,
                    "name": f"Hammer Pro {version}",
                    "html_url": f"{base}/release",
                    "draft": False,
                    "prerelease": False,
                    "body": "## What is new\r\n- Automatic updates: the editor checks GitHub at startup\r\n- **Bold** and `code` are cleaned\r\n- Faster file index\r\n\r\nSee the docs for details.",
                    "assets": [
                        {
                            "name": f"HammerPro-{version}-windows.zip",
                            "browser_download_url": f"{base}/asset.zip",
                            "size": len(payload),
                        }
                    ],
                }
            ).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        elif self.path == "/asset.zip":
            self.send_response(200)
            self.send_header("Content-Type", "application/zip")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            chunk = 256 * 1024
            for i in range(0, len(payload), chunk):
                self.wfile.write(payload[i : i + chunk])
                if delay:
                    time.sleep(delay)
        else:
            self.send_response(404)
            self.end_headers()


ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()

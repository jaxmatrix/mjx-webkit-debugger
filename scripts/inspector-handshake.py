#!/usr/bin/env python3
"""T-000 — prove the WebKitGTK inspector handshake.

WebKitGTK does *not* speak the JSON length-prefixed PlayStation protocol.
On GTK/WPE the server is glib `RemoteInspectorServer` and framing is WTF
`SocketConnection`:

    [u32 body_size BE] [u8 flags] [name\\0] [GVariant body]

`SetupInspectorClient` carries a GVariant `(ay)` — bytestring SHA1 hex digest
of `InspectorBackendCommands.js`. Reply: `DidSetupInspectorClient`, then
`SetTargetList` as `(ta(tsssb))`.

Usage:
    python3 scripts/t000_start_debuggee.py &   # or start MiniBrowser yourself
    python3 scripts/inspector-handshake.py [host:port]
"""

from __future__ import annotations

import hashlib
import socket
import struct
import subprocess
import sys
import time
from ctypes import (
    CDLL,
    POINTER,
    c_char,
    c_char_p,
    c_int,
    c_size_t,
    c_uint64,
    c_void_p,
    create_string_buffer,
    memmove,
)

BYTE_ORDER_LITTLE_ENDIAN = 1 << 0
BACKEND_COMMANDS_PATH = (
    "/org/webkit/inspector/UserInterface/Protocol/InspectorBackendCommands.js"
)
WEBKIT_LIB = "/usr/lib/x86_64-linux-gnu/libwebkit2gtk-4.1.so.0"


class GLib:
    def __init__(self):
        self.lib = CDLL("libglib-2.0.so.0")
        L = self.lib
        L.g_variant_new_bytestring.restype = c_void_p
        L.g_variant_new_bytestring.argtypes = [c_char_p]
        L.g_variant_new_tuple.restype = c_void_p
        L.g_variant_new_tuple.argtypes = [POINTER(c_void_p), c_size_t]
        L.g_variant_ref_sink.restype = c_void_p
        L.g_variant_ref_sink.argtypes = [c_void_p]
        L.g_variant_unref.argtypes = [c_void_p]
        L.g_variant_get_size.restype = c_size_t
        L.g_variant_get_size.argtypes = [c_void_p]
        L.g_variant_get_data.restype = c_void_p
        L.g_variant_get_data.argtypes = [c_void_p]
        L.g_variant_new_from_data.restype = c_void_p
        L.g_variant_new_from_data.argtypes = [
            c_char_p,
            c_void_p,
            c_size_t,
            c_int,
            c_void_p,
            c_void_p,
        ]
        L.g_variant_n_children.restype = c_size_t
        L.g_variant_n_children.argtypes = [c_void_p]
        L.g_variant_get_child_value.restype = c_void_p
        L.g_variant_get_child_value.argtypes = [c_void_p, c_size_t]
        L.g_variant_get_uint64.restype = c_uint64
        L.g_variant_get_uint64.argtypes = [c_void_p]
        L.g_variant_get_string.restype = c_char_p
        L.g_variant_get_string.argtypes = [c_void_p, POINTER(c_size_t)]
        L.g_variant_get_boolean.restype = c_int
        L.g_variant_get_boolean.argtypes = [c_void_p]
        L.g_variant_get_bytestring.restype = c_char_p
        L.g_variant_get_bytestring.argtypes = [c_void_p]

    def bytestring(self, data: bytes) -> c_void_p:
        return c_void_p(self.lib.g_variant_new_bytestring(data))

    def tuple1(self, child: c_void_p) -> c_void_p:
        arr = (c_void_p * 1)(child)
        return c_void_p(self.lib.g_variant_new_tuple(arr, 1))

    def serialize(self, variant: c_void_p) -> bytes:
        v = c_void_p(self.lib.g_variant_ref_sink(variant))
        size = self.lib.g_variant_get_size(v)
        ptr = self.lib.g_variant_get_data(v)
        out = create_string_buffer(size)
        memmove(out, ptr, size)
        self.lib.g_variant_unref(v)
        return out.raw

    def parse(self, type_string: bytes, payload: bytes) -> tuple[c_void_p, object]:
        holder = create_string_buffer(payload)
        v = self.lib.g_variant_new_from_data(
            type_string, holder, len(payload), False, None, None
        )
        if not v:
            raise RuntimeError(f"GVariant parse failed for {type_string!r}")
        v = c_void_p(self.lib.g_variant_ref_sink(c_void_p(v)))
        return v, holder


def backend_commands_hash() -> bytes:
    out = subprocess.check_output(
        ["gresource", "extract", WEBKIT_LIB, BACKEND_COMMANDS_PATH]
    )
    return hashlib.sha1(out).hexdigest().encode("ascii")


def encode_message(name: bytes, body_payload: bytes) -> bytes:
    body = name + b"\0" + body_payload
    return struct.pack("!I", len(body)) + bytes([BYTE_ORDER_LITTLE_ENDIAN]) + body


def try_read_message(buf: bytearray):
    if len(buf) < 4:
        return None
    (body_size,) = struct.unpack("!I", buf[:4])
    if body_size < 2 or body_size > 512 * 1024 * 1024:
        raise RuntimeError(f"implausible body size {body_size}")
    total = 4 + 1 + body_size
    if len(buf) < total:
        return None
    flags = buf[4]
    body = bytes(buf[5:total])
    del buf[:total]
    nul = body.find(b"\0")
    if nul < 0:
        raise RuntimeError("message name missing NUL")
    return body[:nul], flags, body[nul + 1 :]


def main() -> int:
    address = sys.argv[1] if len(sys.argv) > 1 else "127.0.0.1:2999"
    host, port_s = address.rsplit(":", 1)
    port = int(port_s)

    glib = GLib()
    digest = backend_commands_hash()
    print(f"backendCommandsHash = {digest.decode()}", flush=True)

    params = glib.tuple1(glib.bytestring(digest))
    payload = glib.serialize(params)

    sock = socket.create_connection((host, port), timeout=5)
    sock.settimeout(0.5)
    print(f"connected to {address}", flush=True)
    sock.sendall(encode_message(b"SetupInspectorClient", payload))
    print("sent SetupInspectorClient", flush=True)

    buf = bytearray()
    targets = []
    saw_setup = False
    deadline = time.time() + 15

    while time.time() < deadline:
        try:
            chunk = sock.recv(65536)
            if not chunk:
                print("server closed connection", flush=True)
                break
            buf.extend(chunk)
        except socket.timeout:
            pass

        while True:
            msg = try_read_message(buf)
            if msg is None:
                break
            name, flags, payload = msg
            if not (flags & BYTE_ORDER_LITTLE_ENDIAN):
                raise RuntimeError("unexpected big-endian message")
            print(f"<< {name.decode()} ({len(payload)} B)", flush=True)

            if name == b"DidSetupInspectorClient":
                saw_setup = True
                v, holder = glib.parse(b"(ay)", payload)
                child = c_void_p(glib.lib.g_variant_get_child_value(v, 0))
                raw = glib.lib.g_variant_get_bytestring(child) or b""
                print(f"   backendCommands = {len(raw)} bytes", flush=True)
                glib.lib.g_variant_unref(child)
                glib.lib.g_variant_unref(v)
                del holder

            elif name == b"SetTargetList":
                v, holder = glib.parse(b"(ta(tsssb))", payload)
                conn_v = c_void_p(glib.lib.g_variant_get_child_value(v, 0))
                arr_v = c_void_p(glib.lib.g_variant_get_child_value(v, 1))
                connection_id = int(glib.lib.g_variant_get_uint64(conn_v))
                count = int(glib.lib.g_variant_n_children(arr_v))
                print(f"   connectionID={connection_id} targets={count}", flush=True)
                for i in range(count):
                    child = c_void_p(glib.lib.g_variant_get_child_value(arr_v, i))
                    c0 = c_void_p(glib.lib.g_variant_get_child_value(child, 0))
                    c1 = c_void_p(glib.lib.g_variant_get_child_value(child, 1))
                    c2 = c_void_p(glib.lib.g_variant_get_child_value(child, 2))
                    c3 = c_void_p(glib.lib.g_variant_get_child_value(child, 3))
                    c4 = c_void_p(glib.lib.g_variant_get_child_value(child, 4))
                    tid = int(glib.lib.g_variant_get_uint64(c0))
                    typ = (glib.lib.g_variant_get_string(c1, None) or b"").decode()
                    tname = (glib.lib.g_variant_get_string(c2, None) or b"").decode()
                    url = (glib.lib.g_variant_get_string(c3, None) or b"").decode()
                    _local = bool(glib.lib.g_variant_get_boolean(c4))
                    for c in (c0, c1, c2, c3, c4, child):
                        glib.lib.g_variant_unref(c)
                    targets.append(
                        {
                            "connectionID": connection_id,
                            "targetID": tid,
                            "type": typ,
                            "name": tname,
                            "url": url,
                        }
                    )
                    print(
                        f"   [{len(targets)-1}] {typ} {tname!r} {url}",
                        flush=True,
                    )
                glib.lib.g_variant_unref(conn_v)
                glib.lib.g_variant_unref(arr_v)
                glib.lib.g_variant_unref(v)
                del holder
                if targets:
                    sock.close()
                    print("OK — handshake complete", flush=True)
                    return 0

    sock.close()
    if not saw_setup:
        print(
            "FAIL — no DidSetupInspectorClient (wrong protocol or dead server).",
            file=sys.stderr,
        )
        return 2
    print(
        "FAIL — setup ok but no targets. Need developer extras + a loaded page.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())

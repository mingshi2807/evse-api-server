#!/usr/bin/env python3
"""
EVCC Emulator — simulates an EV (EVCC) talking ISO 15118-20 over TCP.

Sends V2GTP-framed EXI payloads to a libiso15118 SECC endpoint.
Use with: evse-api-server (Rust) + libiso15118 (C++).

Usage:
    tools/evcc_emulator.py --scenario scenarios/smoke.json

The scenario file is a JSON array of steps. Each step:
  {"action": "send",    "payload_type": "0x8001", "exi_hex": "01fe..."}
  {"action": "expect",  "payload_type": "0x8001"}
  {"action": "wait",    "duration": 0.5}
  {"action": "close"}
"""

import socket
import struct
import json
import time
import argparse
import sys

V2GTP_HEADER_SIZE = 8
SDP_PROTO_VERSION = 0x01
SDP_INVERSE_PROTO_VERSION = 0xFE


class V2GTPFramer:
    """Minimal V2GTP framer — packs/unpacks SDP payloads."""

    def pack(self, payload_type: int, payload: bytes) -> bytes:
        header = struct.pack(
            "!BBHI",
            SDP_PROTO_VERSION,
            SDP_INVERSE_PROTO_VERSION,
            payload_type,
            len(payload),
        )
        return header + payload

    def unpack_header(self, data: bytes) -> tuple:
        if len(data) < V2GTP_HEADER_SIZE:
            raise ValueError(f"V2GTP header too short: {len(data)} bytes")
        proto, inv_proto, ptype, plen = struct.unpack_from("!BBHI", data)
        if proto != SDP_PROTO_VERSION:
            raise ValueError(f"Bad protocol version: 0x{proto:02x}")
        return ptype, plen


class ScenarioRunner:
    def __init__(self, scenario_path: str, host: str, port: int):
        with open(scenario_path) as f:
            self.steps = json.load(f)
        self.host = host
        self.port = port
        self.sock: socket.socket | None = None
        self.framer = V2GTPFramer()
        self.passed = 0
        self.failed = 0

    def connect(self):
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.sock.settimeout(10.0)
        self.sock.connect((self.host, self.port))
        print(f"[EVCC] Connected to {self.host}:{self.port}")

    def send(self, payload_type: int, exi_hex: str):
        payload = bytes.fromhex(exi_hex)
        packet = self.framer.pack(payload_type, payload)
        self.sock.sendall(packet)
        print(f"[EVCC] SEND  payload_type=0x{payload_type:04x} len={len(payload)}")

    def recv(self) -> tuple[int, bytes]:
        header = b""
        while len(header) < V2GTP_HEADER_SIZE:
            chunk = self.sock.recv(V2GTP_HEADER_SIZE - len(header))
            if not chunk:
                raise ConnectionError("Connection closed by peer")
            header += chunk
        ptype, plen = self.framer.unpack_header(header)
        payload = b""
        while len(payload) < plen:
            chunk = self.sock.recv(plen - len(payload))
            if not chunk:
                break
            payload += chunk
        print(f"[EVCC] RECV  payload_type=0x{ptype:04x} len={len(payload)}")
        return ptype, payload

    def run(self):
        self.connect()

        for i, step in enumerate(self.steps):
            action = step.get("action", "")
            comment = step.get("comment", "")
            if comment:
                print(f"[EVCC] Step {i}: {comment}")

            try:
                if action == "send":
                    ptype = int(step["payload_type"], 0)
                    self.send(ptype, step["exi_hex"])
                    self.passed += 1

                elif action == "expect":
                    expected_ptype = step.get("payload_type")
                    ptype, payload = self.recv()
                    if expected_ptype:
                        exp = int(expected_ptype, 0)
                        if ptype != exp:
                            print(f"[EVCC]   FAIL: expected payload_type 0x{exp:04x}, got 0x{ptype:04x}")
                            self.failed += 1
                        else:
                            self.passed += 1
                    else:
                        self.passed += 1

                elif action == "wait":
                    duration = step.get("duration", 1.0)
                    time.sleep(duration)

                elif action == "close":
                    self.sock.close()
                    print("[EVCC] Connection closed")
                    break

                else:
                    print(f"[EVCC]   WARN: unknown action '{action}'")

            except Exception as e:
                print(f"[EVCC]   ERROR: {e}")
                self.failed += 1
                break

        if self.sock:
            try:
                self.sock.close()
            except Exception:
                pass

        return self.failed == 0


def main():
    parser = argparse.ArgumentParser(description="EVCC emulator for ISO 15118-20 testing")
    parser.add_argument("--scenario", required=True, help="JSON scenario file")
    parser.add_argument("--host", default="127.0.0.1", help="SECC TCP host")
    parser.add_argument("--port", type=int, default=50000, help="SECC TCP port")
    args = parser.parse_args()

    runner = ScenarioRunner(args.scenario, args.host, args.port)
    success = runner.run()

    print(f"\n[EVCC] Done: {runner.passed} passed, {runner.failed} failed")
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()

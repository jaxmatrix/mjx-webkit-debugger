#!/usr/bin/env python3
"""Tiny WebSocket echo server for the network-load fixture page.

Usage:
    python3 fixtures/page/ws_echo.py
    # listens on ws://127.0.0.1:8732
"""
from __future__ import annotations

import asyncio

import websockets


async def echo(websocket: websockets.ServerConnection) -> None:
    async for message in websocket:
        await websocket.send(message)


async def main() -> None:
    async with websockets.serve(echo, "127.0.0.1", 8732):
        print("ws echo on ws://127.0.0.1:8732", flush=True)
        await asyncio.Future()


if __name__ == "__main__":
    asyncio.run(main())

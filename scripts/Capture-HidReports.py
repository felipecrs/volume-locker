"""Read-only Windows HID capture for investigating headset power transitions.

Use only the headset's vendor HID interface, not a keyboard/mouse interface.
No output or feature reports are sent. Captures belong in ignored target/.
"""

import argparse
import ctypes
from ctypes import wintypes
import json
from pathlib import Path
import sys
import time


def capture(path: str, seconds: int, output: Path) -> None:
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    hid = ctypes.WinDLL("hid", use_last_error=True)

    class Overlapped(ctypes.Structure):
        _fields_ = [
            ("Internal", ctypes.c_size_t),
            ("InternalHigh", ctypes.c_size_t),
            ("Offset", wintypes.DWORD),
            ("OffsetHigh", wintypes.DWORD),
            ("hEvent", wintypes.HANDLE),
        ]

    kernel.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD,
                                  wintypes.LPVOID, wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
    kernel.CreateFileW.restype = wintypes.HANDLE
    kernel.CreateEventW.argtypes = [wintypes.LPVOID, wintypes.BOOL, wintypes.BOOL, wintypes.LPCWSTR]
    kernel.CreateEventW.restype = wintypes.HANDLE
    kernel.ReadFile.argtypes = [wintypes.HANDLE, wintypes.LPVOID, wintypes.DWORD,
                               ctypes.POINTER(wintypes.DWORD), ctypes.POINTER(Overlapped)]
    kernel.ReadFile.restype = wintypes.BOOL
    kernel.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
    kernel.WaitForSingleObject.restype = wintypes.DWORD
    kernel.GetOverlappedResult.argtypes = [wintypes.HANDLE, ctypes.POINTER(Overlapped),
                                         ctypes.POINTER(wintypes.DWORD), wintypes.BOOL]
    kernel.GetOverlappedResult.restype = wintypes.BOOL
    kernel.CancelIoEx.argtypes = [wintypes.HANDLE, ctypes.POINTER(Overlapped)]
    kernel.CancelIoEx.restype = wintypes.BOOL
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    kernel.CloseHandle.restype = wintypes.BOOL
    hid.HidD_GetPreparsedData.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.LPVOID)]
    hid.HidD_GetPreparsedData.restype = wintypes.BOOL
    hid.HidD_FreePreparsedData.argtypes = [wintypes.LPVOID]
    hid.HidP_GetCaps.argtypes = [wintypes.LPVOID, wintypes.LPVOID]
    hid.HidP_GetCaps.restype = wintypes.LONG

    handle = kernel.CreateFileW(path, 0x80000000, 3, None, 3, 0x40000000, None)
    if handle == ctypes.c_void_p(-1).value:
        raise ctypes.WinError(ctypes.get_last_error())
    event = None
    try:
        preparsed = wintypes.LPVOID()
        if not hid.HidD_GetPreparsedData(handle, ctypes.byref(preparsed)):
            raise ctypes.WinError(ctypes.get_last_error())
        try:
            caps = ctypes.create_string_buffer(64)
            if hid.HidP_GetCaps(preparsed, caps) < 0:
                raise RuntimeError("Cannot read HID capabilities")
            report_size = int.from_bytes(caps.raw[4:6], "little")
            if not 1 <= report_size <= 4096:
                raise RuntimeError(f"Unexpected input report size: {report_size}")
        finally:
            hid.HidD_FreePreparsedData(preparsed)

        event = kernel.CreateEventW(None, True, False, None)
        if not event:
            raise ctypes.WinError(ctypes.get_last_error())
        output.parent.mkdir(parents=True, exist_ok=True)
        print(f"Listening to headset HID input reports ({report_size} bytes) for {seconds}s.", flush=True)
        deadline = time.monotonic() + seconds
        count = 0
        with output.open("x", encoding="utf-8") as log:
            while time.monotonic() < deadline:
                overlapped = Overlapped(hEvent=event)
                buffer = ctypes.create_string_buffer(report_size)
                size = wintypes.DWORD()
                started = kernel.ReadFile(handle, buffer, report_size, ctypes.byref(size), ctypes.byref(overlapped))
                if not started and ctypes.get_last_error() != 997:  # ERROR_IO_PENDING
                    raise ctypes.WinError(ctypes.get_last_error())
                try:
                    wait = kernel.WaitForSingleObject(event, 1000)
                    if wait == 258:  # WAIT_TIMEOUT
                        continue
                    if wait != 0:
                        raise ctypes.WinError(ctypes.get_last_error())
                    if not kernel.GetOverlappedResult(handle, ctypes.byref(overlapped), ctypes.byref(size), False):
                        raise ctypes.WinError(ctypes.get_last_error())
                    record = {"unix_time": time.time(), "hex": buffer.raw[:size.value].hex()}
                    log.write(json.dumps(record) + "\n")
                    log.flush()
                    count += 1
                    print(record["hex"], flush=True)
                finally:
                    # Cancel and join this handle's read before its buffer goes out of scope.
                    kernel.CancelIoEx(handle, ctypes.byref(overlapped))
                    kernel.GetOverlappedResult(handle, ctypes.byref(overlapped), ctypes.byref(size), True)
        print(f"Captured {count} reports to {output}.", flush=True)
    finally:
        if event:
            kernel.CloseHandle(event)
        kernel.CloseHandle(handle)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--path", required=True, help="Headset vendor HID device path")
    parser.add_argument("--seconds", type=int, default=45)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if sys.platform != "win32":
        parser.error("This diagnostic uses Windows HID APIs")
    if not 1 <= args.seconds <= 60:
        parser.error("Use a capture duration from 1 to 60 seconds")
    capture(args.path, args.seconds, args.output)

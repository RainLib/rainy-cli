#!/usr/bin/env python3
"""Unix PTY regression tests for Rainy's interactive terminal contract."""

from __future__ import annotations

import fcntl
import os
import pty
import select
import shutil
import struct
import subprocess
import sys
import tempfile
import termios
import time
from pathlib import Path


class PtyProcess:
    def __init__(self, argv: list[str], *, columns: int = 80) -> None:
        self.master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, columns, 0, 0))
        env = os.environ.copy()
        env.update(
            {
                "COLUMNS": str(columns),
                "LINES": "24",
                "TERM": "xterm-256color",
                "RAINY_NO_UPDATE_CHECK": "1",
            }
        )
        self.process = subprocess.Popen(
            argv,
            stdin=slave,
            stdout=slave,
            stderr=slave,
            env=env,
            start_new_session=True,
            close_fds=True,
        )
        os.close(slave)
        self.output = bytearray()

    def send(self, value: bytes) -> None:
        os.write(self.master, value)

    def wait_for(self, value: bytes, *, occurrences: int = 1, timeout: float = 10) -> None:
        deadline = time.monotonic() + timeout
        while self.output.count(value) < occurrences and time.monotonic() < deadline:
            self._read(0.1)
        if self.output.count(value) < occurrences:
            raise AssertionError(
                f"PTY output did not contain {value!r} {occurrences} time(s):\n"
                + self.text()
            )

    def finish(self, *, timeout: float = 15) -> tuple[int, bytes]:
        deadline = time.monotonic() + timeout
        while self.process.poll() is None and time.monotonic() < deadline:
            self._read(0.1)
        if self.process.poll() is None:
            os.killpg(self.process.pid, 9)
            self.process.wait()
            raise AssertionError("PTY child did not terminate before the timeout")
        for _ in range(10):
            if not self._read(0.02):
                break
        os.close(self.master)
        return self.process.returncode, bytes(self.output)

    def text(self) -> str:
        return self.output.decode("utf-8", errors="replace")

    def _read(self, timeout: float) -> bool:
        readable, _, _ = select.select([self.master], [], [], timeout)
        if not readable:
            return False
        try:
            chunk = os.read(self.master, 65536)
        except OSError:
            return False
        self.output.extend(chunk)
        return bool(chunk)


def create_project(binary: Path, root: Path, name: str) -> Path:
    subprocess.run(
        [str(binary), "--workspace", str(root), "new", name, "--apply"],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        env={**os.environ, "RAINY_NO_UPDATE_CHECK": "1"},
    )
    return root / name


def test_install_and_progress(binary: Path, root: Path) -> None:
    project = create_project(binary, root, "interactive-install")
    child = PtyProcess(
        [
            str(binary),
            "--workspace",
            str(project),
            "--no-color",
            "--progress",
            "always",
            "skill",
            "install",
        ],
        columns=80,
    )
    child.wait_for(b"Select the Skill bundle")
    child.send(b"\x1b[B\r")  # Rainy-only bundle.
    child.wait_for(b"Select target agent hosts")
    child.send(b"\r")  # Keep the detected/default target.
    child.wait_for(b"Install the selected Skill bundle now?")
    child.send(b"\r")
    code, output = child.finish()
    text = output.decode("utf-8", errors="replace")
    if code != 0:
        raise AssertionError(f"interactive install exited {code}:\n{text}")
    if b"Completed in" not in output:
        raise AssertionError("progress did not resume after the confirmation prompt")
    if b"\x1b[36m" in output or b"\x1b[0m" in output:
        raise AssertionError("--no-color emitted ANSI color sequences")
    if b"\x1b[?25h" not in output:
        raise AssertionError("interactive install did not restore the terminal cursor")
    if not (project / "rainy-skills.yaml").is_file():
        raise AssertionError("interactive install did not write rainy-skills.yaml")
    if not (project / ".agents/skills/rainy-cli/SKILL.md").is_file():
        raise AssertionError("interactive install did not install the universal Rainy Skill")


def test_back_and_interrupt(binary: Path, root: Path) -> None:
    project = create_project(binary, root, "interactive-cancel")
    child = PtyProcess(
        [str(binary), "--workspace", str(project), "skill", "install"], columns=80
    )
    child.wait_for(b"Select the Skill bundle")
    child.send(b"\x1b[B\r")
    child.wait_for(b"Select target agent hosts")
    child.send(b"\x1b")
    child.wait_for(b"Select the Skill bundle", occurrences=2)
    time.sleep(0.25)  # Let Inquire re-enter raw mode after returning a page.
    child.send(b"\x03")
    code, output = child.finish()
    if code != 130:
        raise AssertionError(
            f"Ctrl+C returned {code}, expected 130:\n"
            + output.decode("utf-8", errors="replace")
        )
    if b"CANCELLED" not in output:
        raise AssertionError("Ctrl+C did not produce the stable CANCELLED error code")
    if (project / "rainy-skills.yaml").exists():
        raise AssertionError("cancelled interactive installation changed the workspace")


def test_search_multiselect_and_preview_rejection(binary: Path, root: Path) -> None:
    project = create_project(binary, root, "interactive-search")
    skill = project / "rainy-skills" / "searchable-skill"
    skill.mkdir(parents=True)
    (skill / "SKILL.md").write_text(
        "---\n"
        "name: searchable-skill\n"
        "description: Searchable project skill used by the PTY regression test.\n"
        "---\n\n"
        "# Searchable Skill\n",
        encoding="utf-8",
    )
    child = PtyProcess(
        [str(binary), "--workspace", str(project), "skill", "install"],
        columns=80,
    )
    child.wait_for(b"Select the Skill bundle")
    child.send(b"Rainy\r")
    child.wait_for(b"Select target agent hosts")
    child.send(b"\x1b[DClaude \r")
    child.wait_for(b"Select project Skills")
    child.send(b"searchable \r")
    child.wait_for(b"Install the selected Skill bundle now?")
    child.send(b"n\r")
    code, output = child.finish()
    text = output.decode("utf-8", errors="replace")
    if code != 0 or b"Preview only; no files changed" not in output:
        raise AssertionError(f"rejected installation exited {code}:\n{text}")
    if b"claude" not in output.lower() or b"searchable-skill" not in output:
        raise AssertionError(f"search and multi-selection were not reflected:\n{text}")
    if (project / "rainy-skills.yaml").exists():
        raise AssertionError("rejected interactive installation changed the workspace")


def test_terminal_widths(binary: Path) -> None:
    for columns in (40, 80, 160):
        child = PtyProcess([str(binary), "capability", "list"], columns=columns)
        code, output = child.finish()
        if code != 0 or b"Capabilities" not in output:
            raise AssertionError(f"capability output failed at {columns} columns")


def main() -> int:
    if os.name != "posix":
        print("non-POSIX host; skipping PTY tests")
        return 0
    binary = Path(sys.argv[1] if len(sys.argv) > 1 else "target/debug/rainy").resolve()
    if not binary.is_file():
        raise SystemExit(f"Rainy binary not found: {binary}")
    root = Path(tempfile.mkdtemp(prefix="rainy-tty-"))
    try:
        test_install_and_progress(binary, root)
        test_back_and_interrupt(binary, root)
        test_search_multiselect_and_preview_rejection(binary, root)
        test_terminal_widths(binary)
    finally:
        shutil.rmtree(root, ignore_errors=True)
    print("PTY interaction tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

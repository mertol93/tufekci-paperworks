# SPDX-License-Identifier: AGPL-3.0-or-later
"""Emit bounded machine-readable OCRmyPDF progress for Paperworks."""

from __future__ import annotations

import math
import sys

from ocrmypdf import hookimpl

_MARKER = "PAPERWORKS_OCR_PROGRESS_V1"
_MAX_UNITS = 1_000_000.0


def _number(value):
    try:
        parsed = float(value)
    except (TypeError, ValueError):
        return None
    if not math.isfinite(parsed) or abs(parsed) > _MAX_UNITS:
        return None
    return parsed


class PaperworksProgressBar:
    def __init__(
        self,
        *,
        total=None,
        desc=None,
        unit=None,
        disable=False,
        **kwargs,
    ):
        del unit, kwargs
        self.total = _number(total)
        self.desc = str(desc or "")
        self.disable = bool(disable)
        self.current = 0.0
        self.last_percent = -1

    def __enter__(self):
        self._emit()
        return self

    def __exit__(self, exc_type, exc_value, traceback):
        del exc_value, traceback
        if exc_type is None and self.total is not None:
            self.current = self.total
            self._emit()
        return False

    def update(self, n=1, *, completed=None):
        absolute = _number(completed)
        increment = _number(n)
        if absolute is not None:
            self.current = absolute
        elif increment is not None:
            self.current += increment
        self._emit()

    def _emit(self):
        if (
            self.disable
            or self.desc.strip().casefold() != "ocr"
            or self.total is None
            or self.total <= 0
        ):
            return
        current = min(max(self.current, 0.0), self.total)
        percent = min(100, max(0, int(current * 100.0 / self.total)))
        if percent <= self.last_percent:
            return
        self.last_percent = percent
        sys.stderr.write(
            f"{_MARKER}\t{percent}\t{current:.6g}\t{self.total:.6g}\n"
        )
        sys.stderr.flush()


@hookimpl(trylast=True)
def check_options(options):
    # OCRmyPDF disables progress for piped stderr; the app consumes bounded records.
    options.progress_bar = True


@hookimpl
def get_progressbar_class():
    return PaperworksProgressBar

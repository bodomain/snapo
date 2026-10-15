"""Sync queued app changes in the foreground or a user service."""
import argparse
import logging
from pathlib import Path
import signal
import threading
import math

from database import Config, Store


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--once", action="store_true", help="Sync once and exit")
    parser.add_argument("--interval", type=float, default=15, help="Seconds between syncs")
    parser.add_argument("--db", type=Path)
    parser.add_argument("--local", action="store_true")
    args = parser.parse_args()
    if not math.isfinite(args.interval) or args.interval <= 0:
        parser.error("--interval must be positive and finite")
    if args.local and args.db is None:
        parser.error("--local requires --db")
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(message)s")
    store = Store(Config.load(args.db, args.local))
    if args.once:
        store.sync()
        logging.info("Sync complete")
        return
    stop = threading.Event()
    for signum in (signal.SIGTERM, signal.SIGINT):
        signal.signal(signum, lambda *_: stop.set())
    logging.info("Sync worker started (interval: %s seconds)", args.interval)
    failed = False
    first = True
    while not stop.is_set():
        try:
            store.sync()
            if first or failed:
                logging.info("Sync complete; connection available")
            first = failed = False
        except Exception as error:
            if not failed:
                message = str(error)
                if store.config.token:
                    message = message.replace(store.config.token, "<REDACTED>")
                logging.warning("Sync failed; will retry: %s", message)
            failed = True
        stop.wait(args.interval)


if __name__ == "__main__":
    main()

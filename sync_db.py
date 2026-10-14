"""Keep the local Turso replica synced while the timer is closed."""

import argparse
import logging
import signal
import socket
import threading

import database


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--once", action="store_true", help="Sync once and exit")
    parser.add_argument("--interval", type=float, default=15, help="Seconds between syncs")
    args = parser.parse_args()
    if args.interval <= 0:
        parser.error("--interval must be positive")

    # Bound network waits so an unavailable server does not stall replica access.
    socket.setdefaulttimeout(5)
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(message)s")
    if args.once:
        database.sync_once()
        logging.info("Turso sync complete")
        return

    stop = threading.Event()
    for signum in (signal.SIGTERM, signal.SIGINT):
        signal.signal(signum, lambda *_: stop.set())

    logging.info("Turso sync worker started (interval: %s seconds)", args.interval)
    failed = False
    first_sync = True
    while not stop.is_set():
        try:
            database.sync_once()
            if failed:
                logging.info("Turso connection restored; sync complete")
            elif first_sync:
                logging.info("Initial Turso sync complete")
            failed = False
            first_sync = False
        except Exception as error:
            if not failed:
                logging.warning("Turso sync failed; will retry: %s", error)
            failed = True
        stop.wait(args.interval)


if __name__ == "__main__":
    main()

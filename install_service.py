"""Generate/install a sync service using this checkout's actual paths."""
import argparse
from pathlib import Path
import subprocess
import sys


def unit(project, python, db, *, local=False):
    def quote(value):
        value = str(value)
        if any(c in value for c in '\n\r\x00'):
            raise ValueError("Service paths cannot contain newlines or NUL bytes")
        return '"' + value.replace('\\', '\\\\').replace('"', '\\"').replace('%', '%%').replace('$', '$$') + '"'
    command = f"{quote(python)} -u {quote(project / 'sync_db.py')} --db {quote(db)}"
    if local:
        command += " --local"
    return f'''[Unit]
Description=Snapo background database sync

[Service]
Type=simple
ExecStart={command}
Restart=on-failure
RestartSec=15
TimeoutStopSec=15

[Install]
WantedBy=default.target
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--db", type=Path)
    parser.add_argument("--install", action="store_true", help="Write and enable the user service; default prints it")
    args = parser.parse_args()
    from database import Config
    config = Config.load(args.db)
    project = Path(__file__).resolve().parent
    content = unit(project, Path(sys.executable).absolute(), config.path, local=config.local)
    if not args.install:
        print(content, end="")
        return
    from database import Store
    Store(config).initialize()  # Refuse to install against an unmigrated database.
    target = Path.home() / ".config/systemd/user/prodz-sync.service"
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.is_symlink():
        target.unlink()
    target.write_text(content)
    subprocess.run(["systemctl", "--user", "daemon-reload"], check=True)
    subprocess.run(["systemctl", "--user", "enable", "prodz-sync.service"], check=True)
    subprocess.run(["systemctl", "--user", "restart", "prodz-sync.service"], check=True)
    print(f"Installed {target}")


if __name__ == "__main__":
    main()

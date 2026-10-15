"""Immutable session history shared by the terminal interfaces.

Revisions are append-only. Replication is set union, so arrival order cannot
silently discard an edit. Only explicit resolution can consume multiple heads.
"""
from dataclasses import asdict, dataclass, replace
from datetime import date, datetime
import csv
import json
import math
import uuid


class ConflictError(ValueError):
    """The editor's base revisions are no longer current."""


@dataclass(frozen=True)
class Session:
    date: str
    timestamp: str
    activity: str
    duration_minutes: float
    comment: str = ""

    def validated(self):
        date.fromisoformat(self.date)
        datetime.fromisoformat(self.timestamp)
        if not self.activity.strip():
            raise ValueError("Activity is required")
        if not math.isfinite(self.duration_minutes) or self.duration_minutes <= 0:
            raise ValueError("Duration must be a positive, finite number")
        return replace(self, activity=self.activity.strip())

    @classmethod
    def now(cls, activity, duration_minutes, comment=""):
        now = datetime.now().astimezone()
        return cls(now.date().isoformat(), now.isoformat(), activity, duration_minutes, comment)


@dataclass(frozen=True)
class Revision:
    revision_id: str
    session_id: str
    parents: tuple[str, ...]
    device_id: str
    created_at: str
    session: Session
    deleted: bool = False
    legacy_id: int | None = None

    def record(self):
        return (self.revision_id, self.session_id, json.dumps(self.parents), self.device_id,
                self.created_at, json.dumps(asdict(self.session)), int(self.deleted), self.legacy_id)

    @classmethod
    def from_record(cls, row):
        rid, sid, parents, device, created, payload, deleted, legacy = row
        return cls(rid, sid, tuple(json.loads(parents)), device, created,
                   Session(**json.loads(payload)), bool(deleted), legacy)


@dataclass(frozen=True)
class HistoryEntry:
    session_id: str
    heads: tuple[Revision, ...]

    @property
    def conflicted(self):
        return len(self.heads) != 1

    @property
    def current(self):
        if self.conflicted:
            raise ConflictError("Resolve the competing versions first")
        return self.heads[0]

    @property
    def head_ids(self):
        return tuple(r.revision_id for r in self.heads)


def reconstruct(revisions):
    grouped = {}
    for revision in revisions:
        grouped.setdefault(revision.session_id, {})[revision.revision_id] = revision
    entries = []
    for sid, versions in grouped.items():
        consumed = {parent for r in versions.values() for parent in r.parents}
        heads = tuple(sorted((r for rid, r in versions.items() if rid not in consumed),
                             key=lambda r: r.revision_id))
        if heads:
            entries.append(HistoryEntry(sid, heads))
    return sorted(entries, key=lambda e: (max(r.session.timestamp for r in e.heads), e.session_id), reverse=True)


class SessionHistory:
    """SQL adapter. The caller owns locking, commits, and connection lifetime."""
    def __init__(self, connection, device_id):
        self.connection = connection
        self.device_id = device_id

    def revisions(self):
        return [Revision.from_record(row) for row in self.connection.execute(
            "SELECT revision_id,session_id,parents,device_id,created_at,payload,deleted,legacy_id "
            "FROM session_revisions").fetchall()]

    def entries(self):
        return reconstruct(self.revisions())

    def get(self, session_id):
        for entry in self.entries():
            if entry.session_id == session_id:
                return entry
        raise KeyError("Session no longer exists")

    def append(self, revision):
        existing = self.connection.execute(
            "SELECT revision_id,session_id,parents,device_id,created_at,payload,deleted,legacy_id "
            "FROM session_revisions WHERE revision_id=?", (revision.revision_id,)).fetchone()
        if existing:
            if Revision.from_record(existing) != revision:
                raise ValueError("Revision ID reused with different contents")
            return
        self.connection.execute("INSERT INTO session_revisions VALUES (?,?,?,?,?,?,?,?)", revision.record())

    def create(self, session, *, session_id=None):
        sid = session_id or str(uuid.uuid4())
        if any(e.session_id == sid for e in self.entries()):
            raise ValueError("Session ID already exists")
        revision = Revision(str(uuid.uuid4()), sid, (), self.device_id,
                            datetime.now().astimezone().isoformat(), session.validated())
        self.append(revision)
        return revision

    def change(self, session_id, base_heads, *, session=None, deleted=None, resolve=False):
        current = self.get(session_id)
        if set(base_heads) != set(current.head_ids):
            raise ConflictError("This session changed. Reopen it and review the latest versions.")
        if current.conflicted and not resolve:
            raise ConflictError("Resolve the competing versions first")
        if resolve and (session is None or deleted is None):
            raise ValueError("Resolution requires session contents and deletion state")
        source = current.heads[0]
        revision = Revision(str(uuid.uuid4()), session_id, tuple(sorted(current.head_ids)), self.device_id,
                            datetime.now().astimezone().isoformat(),
                            session.validated() if session is not None else source.session,
                            source.deleted if deleted is None else deleted, source.legacy_id)
        self.append(revision)
        return revision


def filtered(entries, *, search="", activity="", since="", until="", view="history"):
    if since:
        date.fromisoformat(since)
    if until:
        date.fromisoformat(until)
    if since and until and since > until:
        raise ValueError("Start date must be before end date")
    result = []
    for entry in entries:
        if view == "conflicts":
            visible = entry.conflicted
        elif view == "trash":
            visible = not entry.conflicted and entry.current.deleted
        else:
            visible = entry.conflicted or not entry.current.deleted
        if not visible:
            continue
        if any((not since or r.session.date >= since)
               and (not until or r.session.date <= until)
               and (not activity or activity.casefold() in r.session.activity.casefold())
               and (not search or search.casefold() in
                    (r.session.activity + " " + r.session.comment + " " + r.session.timestamp).casefold())
               for r in entry.heads):
            result.append(entry)
    return result


def totals(entries):
    daily, weekly = {}, {}
    for entry in entries:
        if entry.conflicted or entry.current.deleted:
            continue
        session = entry.current.session
        day = date.fromisoformat(session.date)
        year, week, _ = day.isocalendar()
        for mapping, period in ((daily, session.date), (weekly, f"{year}-W{week:02d}")):
            key = (period, session.activity)
            mapping[key] = mapping.get(key, 0) + session.duration_minutes
    return daily, weekly


def export_csv(entries, path):
    # Exclusive creation prevents an accidental overwrite of a database or report.
    with open(path, "x", newline="", encoding="utf-8") as handle:
        writer = csv.writer(handle)
        writer.writerow(["session_id", "date", "timestamp", "activity", "duration_minutes", "comment"])
        for entry in entries:
            if entry.conflicted or entry.current.deleted:
                continue
            s = entry.current.session
            # Protect spreadsheet consumers from formula injection in free text.
            def cell(value):
                return "'" + value if value.startswith(("=", "+", "-", "@", "\t", "\r")) else value
            writer.writerow([entry.session_id, s.date, s.timestamp, cell(s.activity),
                             s.duration_minutes, cell(s.comment)])

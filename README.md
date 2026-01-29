# prodzCLI

A simple, lightweight, and customizable productivity timer for the command line, written in Python. It helps you manage your time (using the Pomodoro Technique), tracks your activities, and visualizes your daily productivity.

## Features

-   **Customizable Timers:** Set your own durations for work sessions, short breaks, and long breaks.
-   **Activity Tracking:** Log your sessions (activity name, duration, date) to a local SQLite database.
-   **Interactive Controls:** Pause, resume, or exit sessions directly from the terminal.
-   **Notification Sounds:** Plays a notification sound (`bell.wav`) at the end of each session using system audio tools (`aplay`, `paplay`, or `ffplay`). Falls back to the terminal bell if audio is unavailable.
-   **Statistics Visualization:** View a simple ASCII bar chart of your daily productivity directly in the CLI.
-   **Zero Dependencies:** Built using Python's standard library (no `pip install` required).

## Prerequisites

-   **Python 3.6+**
-   **Linux** (Uses `termios`/`tty` for input handling and system tools for audio)
-   **Audio Player:** One of `aplay` (ALSA), `paplay` (PulseAudio), or `ffplay` (FFmpeg) should be installed on your system for sound notifications.

## Installation

1.  Clone this repository or download the source code.
2.  Navigate to the project directory:
    ```bash
    cd snapo
    ```
3.  Ensure `prodz_cli.py` and `bell.wav` are in the same directory.

## Usage

Run the timer using Python:

```bash
python prodz_cli.py [options]
```

### Command-line Arguments

| Argument | Long Flag | Description | Default |
| :--- | :--- | :--- | :--- |
| `-w` | `--work` | Duration of the work session (minutes) | 25 |
| `-b` | `--break` | Duration of the short break (minutes) | 5 |
| `-lb` | `--long-break` | Duration of the long break (minutes) | 15 |
| `-c` | `--cycles` | Number of work cycles before a long break/finish | 4 |
| `-a` | `--activity` | Name of the activity/task being tracked | "whatever" |
| `-h` | `--help` | Show the help message and exit | - |

### Interactive Controls

While the timer is running, you can use the following keys:

-   **`p`**: **Pause** or **Resume** the timer.
-   **`x`**: **Log & Exit**. Stops the timer, saves the elapsed time to the database, and exits.
-   **`q`**: **Quit**. Exits the application immediately *without* saving the session.

### Example

Start a session for "Python Study" with 45-minute work intervals:

```bash
python prodz_cli.py -w 45 -a "Python Study"
```

## Statistics

Your completed sessions (and partially completed ones logged via `x`) are stored in `prodz.db`. To view a graph of your daily productivity:

```bash
python plot.py
```

**Output Example:**

```text
Daily Activity (Duration in Minutes)

2023-10-27 | ████████████ 120.00m
2023-10-28 | ██████ 60.50m
2023-10-29 | ██████████████████ 180.00m
```

## Project Structure

-   `prodz_cli.py`: Main application logic.
-   `database.py`: Handles SQLite database operations.
-   `plot.py`: Generates the statistics chart.
-   `bell.wav`: Notification sound file.
-   `prodz.db`: (Created on first run) Stores session data.

## License

Free to use and modify.
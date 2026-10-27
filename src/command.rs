//! A command-line input for the focus panel. The syntax mirrors the
//! original Python CLI (`prodz -a activity -w 25 -b 5 -lb 15 -c 4 -e -t note`),
//! so remembered flags keep working inside the desktop app.

use anyhow::{Result, bail, ensure};

/// Everything a command may define; `None` leaves the panel value untouched.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FocusCommand {
    pub activity: Option<String>,
    pub comment: Option<String>,
    pub work: Option<u32>,
    pub short_break: Option<u32>,
    pub long_break: Option<u32>,
    pub cycles: Option<u32>,
    pub endless: Option<bool>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Flag {
    Activity,
    Text,
    Work,
    ShortBreak,
    LongBreak,
    Cycles,
    Endless,
}

fn resolve(name: &str) -> Option<Flag> {
    match name {
        "-a" | "--activity" => Some(Flag::Activity),
        "-t" | "--text" => Some(Flag::Text),
        "-w" | "--work" => Some(Flag::Work),
        "-b" | "--break" => Some(Flag::ShortBreak),
        "-lb" | "--long-break" => Some(Flag::LongBreak),
        "-c" | "--cycles" => Some(Flag::Cycles),
        "-e" | "--endless" => Some(Flag::Endless),
        _ => None,
    }
}

const OPTIONS: &str = "-a, -t, -w, -b, -lb, -c und -e sind erlaubt.";

struct Token {
    text: String,
    quoted: bool,
}

/// Split on whitespace while keeping `"Notiz mit Leerzeichen"` as one token.
fn tokenize(input: &str) -> Result<Vec<Token>> {
    let mut tokens = vec![];
    let mut token = String::new();
    let mut quoted = false;
    let mut started = false;
    let mut opened: Option<char> = None;
    for ch in input.chars() {
        if let Some(mark) = opened {
            if ch == mark {
                opened = None;
            } else {
                token.push(ch);
            }
        } else if ch.is_whitespace() {
            if started {
                tokens.push(Token {
                    text: token,
                    quoted,
                });
                token = String::new();
                quoted = false;
                started = false;
            }
        } else if ch == '"' || ch == '\'' {
            opened = Some(ch);
            quoted = true;
            started = true;
        } else {
            token.push(ch);
            started = true;
        }
    }
    if opened.is_some() {
        bail!("Unvollständige Anführungszeichen.");
    }
    if started {
        tokens.push(Token {
            text: token,
            quoted,
        });
    }
    Ok(tokens)
}

fn amount(token: &Token, name: &str) -> Result<u32> {
    match token.text.parse::<u32>() {
        Ok(minutes) => Ok(minutes),
        Err(_) => bail!("Die Option '{name}' erwartet eine Zahl in Minuten."),
    }
}

fn range(name: &str, minutes: u32, minimum: u32, maximum: u32) -> Result<u32> {
    ensure!(
        (minimum..=maximum).contains(&minutes),
        format!("'{name}' muss zwischen {minimum} und {maximum} liegen.")
    );
    Ok(minutes)
}

pub fn parse(input: &str) -> Result<FocusCommand> {
    let tokens = tokenize(input)?;
    let mut command = FocusCommand::default();
    let mut activity: Vec<String> = vec![];
    let mut index = 0;
    let mut seen_flag = false;
    while index < tokens.len() {
        let token = &tokens[index];
        let name = token.text.as_str();
        if !token.quoted && name.starts_with('-') && name.len() > 1 {
            seen_flag = true;
            let (name, inline) = match name.split_once('=') {
                Some((name, value)) => (name, Some(value)),
                None => (name, None),
            };
            let Some(flag) = resolve(name) else {
                bail!("Unbekannte Option '{name}'. {OPTIONS}");
            };
            if flag == Flag::Endless {
                ensure!(inline.is_none(), "Die Option '{name}' nimmt keinen Wert.");
                command.endless = Some(true);
                index += 1;
                continue;
            }
            let value = match inline {
                Some(value) => Token {
                    text: value.into(),
                    quoted: true,
                },
                None => {
                    let Some(next) = tokens.get(index + 1) else {
                        bail!("Die Option '{name}' erwartet einen Wert. {OPTIONS}");
                    };
                    if !next.quoted && next.text.starts_with('-') && next.text.len() > 1 {
                        bail!("Die Option '{name}' erwartet einen Wert. {OPTIONS}");
                    }
                    index += 1;
                    Token {
                        text: next.text.clone(),
                        quoted: next.quoted,
                    }
                }
            };
            match flag {
                Flag::Activity => command.activity = Some(value.text),
                Flag::Text => command.comment = Some(value.text),
                Flag::Work => command.work = Some(amount(&value, name)?),
                Flag::ShortBreak => command.short_break = Some(amount(&value, name)?),
                Flag::LongBreak => command.long_break = Some(amount(&value, name)?),
                Flag::Cycles => command.cycles = Some(amount(&value, name)?),
                Flag::Endless => unreachable!("ist bereits behandelt"),
            }
            index += 1;
            continue;
        }
        // A positional word becomes the activity: any words freely before the
        // first flag, or a single one right after it (`-w 30 Lernen`).
        if seen_flag && !activity.is_empty() {
            bail!("Unbekannte Angabe '{name}'. Mehrere Wörter bitte in Anführungszeichen setzen.");
        }
        activity.push(token.text.clone());
        command.activity = Some(activity.join(" "));
        index += 1;
    }
    if let Some(minutes) = command.work {
        command.work = Some(range("-w", minutes, 1, 240)?);
    }
    if let Some(minutes) = command.short_break {
        command.short_break = Some(range("-b", minutes, 1, 120)?);
    }
    if let Some(minutes) = command.long_break {
        command.long_break = Some(range("-lb", minutes, 1, 180)?);
    }
    if let Some(cycles) = command.cycles {
        command.cycles = Some(range("-c", cycles, 1, 24)?);
    }
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_flags_of_the_python_cli() {
        let command =
            parse("-a Projektarbeit -w 45 -b 10 -lb 20 -c 8 -e -t \"Erste Notiz\"").unwrap();
        assert_eq!(
            command,
            FocusCommand {
                activity: Some("Projektarbeit".into()),
                comment: Some("Erste Notiz".into()),
                work: Some(45),
                short_break: Some(10),
                long_break: Some(20),
                cycles: Some(8),
                endless: Some(true),
            }
        );
        let command =
            parse("--activity Lesen --work=30 --break 5 --long-break 15 --cycles 2 --text Notiz")
                .unwrap();
        assert_eq!(command.activity.as_deref(), Some("Lesen"));
        assert_eq!(command.work, Some(30));
        assert_eq!(command.short_break, Some(5));
        assert_eq!(command.long_break, Some(15));
        assert_eq!(command.cycles, Some(2));
        assert_eq!(command.comment.as_deref(), Some("Notiz"));
    }

    #[test]
    fn activity_may_be_positional_and_quotes_keep_spaces() {
        assert_eq!(parse("Lernen").unwrap().activity.as_deref(), Some("Lernen"));
        assert_eq!(
            parse("Fokus üben -w 30").unwrap().activity.as_deref(),
            Some("Fokus üben")
        );
        assert_eq!(
            parse("'Schwere Aufgabe' -w 15")
                .unwrap()
                .activity
                .as_deref(),
            Some("Schwere Aufgabe")
        );
        assert_eq!(
            parse("-a 'Schwere Aufgabe' -t \"Zwei Worte\"").unwrap(),
            FocusCommand {
                activity: Some("Schwere Aufgabe".into()),
                comment: Some("Zwei Worte".into()),
                ..Default::default()
            }
        );
    }

    #[test]
    fn empty_input_leaves_panel_values_untouched() {
        assert_eq!(parse("").unwrap(), FocusCommand::default());
        assert_eq!(parse("   ").unwrap(), FocusCommand::default());
    }

    #[test]
    fn rejects_unknown_flags_and_bad_values() {
        for (input, expected) in [
            ("-x 5", "Unbekannte Option '-x'"),
            ("-w 0", "'-w' muss zwischen 1 und 240 liegen."),
            ("-w 241", "'-w' muss zwischen 1 und 240 liegen."),
            ("-b 121", "'-b' muss zwischen 1 und 120 liegen."),
            ("-lb 181", "'-lb' muss zwischen 1 und 180 liegen."),
            ("-c 25", "'-c' muss zwischen 1 und 24 liegen."),
            ("-w halb", "'-w' erwartet eine Zahl in Minuten."),
            ("-w", "'-w' erwartet einen Wert"),
            ("-t -w 5", "'-t' erwartet einen Wert"),
            ("Lernen -w 30 extra", "Unbekannte Angabe 'extra'"),
            ("Lernen -w 30 extra noch", "Unbekannte Angabe 'extra'"),
            (
                "--endless=rest",
                "Die Option '--endless' nimmt keinen Wert.",
            ),
            ("'ohne Ende", "Unvollständige Anführungszeichen."),
        ] {
            let error = parse(input).unwrap_err().to_string();
            assert!(
                error.contains(expected),
                "Falsche Meldung für '{input}': {error}"
            );
        }
    }

    #[test]
    fn short_flags_accept_wrote_values_out_of_positional_order() {
        let command = parse("-w 30 Lernen").unwrap();
        assert_eq!(command.activity.as_deref(), Some("Lernen"));
        assert_eq!(command.work, Some(30));
        let command = parse("-t \"Uni Vokabeln\" -e").unwrap();
        assert_eq!(command.comment.as_deref(), Some("Uni Vokabeln"));
        assert_eq!(command.endless, Some(true));
    }
}

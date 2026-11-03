//! Composes the status icon state from Module lifecycle, Module contributions and the
//! config notice. The composer knows no Module by name.

use navori_protocol::{ModuleInfo, ModuleState, TrayGlyph, TrayState};

/// What one Module asks of the status icon, derived from its published State.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrayContribution {
    /// A glyph to show and the sentence that explains it.
    pub glyph: Option<(TrayGlyph, String)>,
    /// A clause saying something waits for the user; its presence turns attention on.
    pub attention: Option<String>,
}

const NORMAL_SENTENCE: &str = "Navori";

/// Ranks the entries into one [`TrayState`]. `entries` are in registration order. A
/// Module's contribution counts only while it is running, because its last published State
/// outlives it. `notice` is the config notice as `(title, message)`; it only ever adds to
/// the summary and never changes the glyph.
pub fn compose_tray(
    entries: &[(ModuleInfo, TrayContribution)],
    notice: Option<(&str, &str)>,
) -> TrayState {
    let running: Vec<&TrayContribution> = entries
        .iter()
        .filter(|(info, _)| info.state == ModuleState::Running)
        .map(|(_, contribution)| contribution)
        .collect();

    let mut problems: Vec<&str> = Vec::new();
    let lifecycle = lifecycle_sentence(entries);
    if let Some(sentence) = &lifecycle {
        problems.push(sentence);
    }
    problems.extend(running.iter().filter_map(|c| match &c.glyph {
        Some((TrayGlyph::Problem, sentence)) => Some(sentence.as_str()),
        _ => None,
    }));

    let first_of = |wanted: TrayGlyph| {
        running.iter().find_map(|c| match &c.glyph {
            Some((glyph, sentence)) if *glyph == wanted => Some(sentence.as_str()),
            _ => None,
        })
    };

    let mut pieces: Vec<String> = Vec::new();
    let glyph = if !problems.is_empty() {
        pieces.extend(problems.into_iter().map(str::to_owned));
        TrayGlyph::Problem
    } else if let Some(sentence) = first_of(TrayGlyph::Lease) {
        pieces.push(sentence.to_owned());
        TrayGlyph::Lease
    } else if let Some(sentence) = first_of(TrayGlyph::AiRunning) {
        pieces.push(sentence.to_owned());
        TrayGlyph::AiRunning
    } else {
        pieces.push(NORMAL_SENTENCE.to_owned());
        TrayGlyph::Normal
    };

    let clause = running.iter().find_map(|c| c.attention.as_deref());
    if let Some(clause) = clause {
        pieces.push(clause.to_owned());
    }
    if let Some((title, message)) = notice {
        pieces.push(format!("{title}. {message}"));
    }

    TrayState {
        glyph,
        attention: clause.is_some(),
        summary: join_pieces(&pieces),
    }
}

/// The first failed Module's sentence, else the first restarting Module's, else none.
fn lifecycle_sentence(entries: &[(ModuleInfo, TrayContribution)]) -> Option<String> {
    let failed = entries.iter().find_map(|(info, _)| match &info.state {
        ModuleState::Failed { last_error } => {
            let detail = if last_error.is_empty() {
                "unknown error"
            } else {
                last_error
            };
            Some(format!("{} stopped: {detail}", info.id))
        }
        _ => None,
    });
    failed.or_else(|| {
        entries.iter().find_map(|(info, _)| match info.state {
            ModuleState::Restarting { .. } => Some(format!("{} is restarting", info.id)),
            _ => None,
        })
    })
}

/// Every piece but the last loses one trailing `.`; they join with `. `.
fn join_pieces(pieces: &[String]) -> String {
    let last = pieces.len().saturating_sub(1);
    pieces
        .iter()
        .enumerate()
        .map(|(i, piece)| {
            if i < last {
                piece.strip_suffix('.').unwrap_or(piece)
            } else {
                piece
            }
        })
        .collect::<Vec<_>>()
        .join(". ")
}

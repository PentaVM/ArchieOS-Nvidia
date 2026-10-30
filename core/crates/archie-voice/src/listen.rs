//! What was heard, turned into what the floor decides on.
//!
//! The speech plane's [`FloorPolicy`](archie_speech::FloorPolicy) decides whether Archie
//! speaks from a [`ConversationalFloorState`]. The learned estimator that will fill that
//! state from the audio is not trained yet, so this fills it from the evidence the ears do
//! have — the words, whether the recogniser called the utterance finished, how long the
//! silence after it ran — and writes that evidence into the state, so that a person asking
//! why Archie answered (or did not) can read it.
//!
//! Two judgements carry most of the weight, and both are deliberately conservative:
//!
//! - **Was it said to Archie?** A microphone in a room hears conversations that are not
//!   addressed to anybody's computer. An utterance counts as addressed when it names Archie,
//!   or when it follows something Archie said closely enough to be the reply to it. Everything
//!   else leaves the response utility low, and the policy stays silent.
//! - **Was it only an acknowledgement?** "ya", "ok", "mhm" answer nothing and ask nothing.
//!   While Archie speaks they must not stop it; after it speaks they must not start it again.

use crate::addressee::AddresseeCues;
use archie_speech::{
    ConversationalFloorState, InterruptionDirection, OverlapState, Probability, SilenceScope,
    Speaker,
};
use std::time::Duration;

/// How long after Archie finished speaking a reply needs no name to be addressed to it.
pub const FOLLOW_UP_WINDOW: Duration = Duration::from_secs(10);

/// The spellings a recogniser gives the name. Nemotron writes what it hears; "Archi" and
/// "Archy" are the same word said aloud.
const NAMES: &[&str] = &["archie", "archi", "archy"];

/// What the Nemotron 3.5 recogniser writes for the name said at the start of a Spanish
/// request ("Archie, dibuja la Mona Lisa" came back "AC dibuja la Mona Lisa").
const ADDRESSED_AS: &[&str] = &["ac", "hachi"];

/// Real words one letter from the name, which must not wake Archie.
const NOT_NAMES: &[&str] = &["arch", "archive", "archer", "arco", "marchi"];

/// Letters to turn `a` into `b`, one insertion, deletion or change at a time.
fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (above + 1)
                .min(row[j] + 1)
                .min(diagonal + usize::from(ca != *cb));
            diagonal = above;
        }
    }
    row[b.len()]
}

/// Whether one written word is the name as a recogniser mishears it: "Archier", "Arshi",
/// "Harchi". Only words that start like the name count, so "marchi" or "ancho" never do.
fn sounds_like_archie(word: &str) -> bool {
    if NAMES.contains(&word) {
        return true;
    }
    if NOT_NAMES.contains(&word) || !(4..=8).contains(&word.chars().count()) {
        return false;
    }
    let starts_like = ["ar", "har", "ahr", "ars"]
        .iter()
        .any(|start| word.starts_with(start));
    starts_like && (edit_distance(word, "archi") <= 1 || edit_distance(word, "archie") <= 1)
}

/// Utterances that acknowledge and ask nothing, compared without accents or punctuation.
const ACKNOWLEDGEMENTS: &[&str] = &[
    "ya",
    "ok",
    "okay",
    "oka",
    "mhm",
    "mm",
    "aja",
    "aha",
    "si",
    "vale",
    "listo",
    "dale",
    "perfecto",
    "bueno",
    "claro",
    "ya po",
    "si po",
    "ah ya",
    "ok gracias",
];

/// Lower case, without accents or punctuation, words separated by single spaces.
pub fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            other => other,
        };
        if c.is_alphanumeric() || c == 'ñ' {
            out.push(c);
        } else if !out.ends_with(' ') && !out.is_empty() {
            out.push(' ');
        }
    }
    out.trim_end().to_string()
}

/// Whether the utterance names Archie — also when the recogniser misspelled the name or
/// split it in two ("ar chi").
pub fn names_archie(text: &str) -> bool {
    let plain = plain(text);
    let words: Vec<&str> = plain.split(' ').collect();
    // Said first, in Spanish, the name comes back as "AC" (two letters, "a-ce"): only at
    // the head of a request, where nobody starts a sentence with those letters alone.
    if words.len() > 2 && ADDRESSED_AS.contains(&words[0]) {
        return true;
    }
    words.iter().any(|word| sounds_like_archie(word))
        || words.windows(2).any(|pair| {
            pair[0].len() <= 3 && sounds_like_archie(&format!("{}{}", pair[0], pair[1]))
        })
}

/// Whether the utterance is only an acknowledgement.
pub fn is_acknowledgement(text: &str) -> bool {
    let plain = plain(text);
    !plain.is_empty() && ACKNOWLEDGEMENTS.contains(&plain.as_str())
}

/// One finished utterance, as the ears report it.
#[derive(Debug, Clone, PartialEq)]
pub struct Utterance {
    pub text: String,
    /// The recogniser marked it final: it heard the end of the turn, not a breath.
    pub final_by_recogniser: bool,
    /// How long it has been quiet since the last word.
    pub silence_ms: u32,
}

/// The floor after an utterance, with nobody speaking.
///
/// `since_archie_spoke` is how long ago Archie last finished saying something, if it has.
pub fn after_utterance(
    heard: &Utterance,
    since_archie_spoke: Option<Duration>,
) -> ConversationalFloorState {
    after_utterance_within(heard, since_archie_spoke, FOLLOW_UP_WINDOW)
}

/// As [`after_utterance`], with the follow-up window given: how long after Archie spoke a
/// reply needs no name.
pub fn after_utterance_within(
    heard: &Utterance,
    since_archie_spoke: Option<Duration>,
    follow_up: Duration,
) -> ConversationalFloorState {
    after_utterance_with(
        heard,
        since_archie_spoke,
        follow_up,
        &AddresseeCues::default(),
    )
}

/// As [`after_utterance_within`], with what is known around the words: who else is there,
/// where the speaker looked, whether someone else answered. See [`crate::addressee`].
pub fn after_utterance_with(
    heard: &Utterance,
    since_archie_spoke: Option<Duration>,
    follow_up: Duration,
    cues: &AddresseeCues,
) -> ConversationalFloorState {
    let replying = since_archie_spoke.is_some_and(|since| since <= follow_up);
    let addressee = crate::addressee::estimate(&heard.text, replying, cues);
    let mut evidence = addressee.evidence;

    let acknowledgement = is_acknowledgement(&heard.text);
    if acknowledgement {
        evidence.push("the words are only an acknowledgement".to_string());
    }

    let trimmed = heard.text.trim_end();
    let sounds_finished = trimmed.ends_with(['.', '?', '!', '¿', '¡']) || heard.final_by_recogniser;
    let trailing = plain(trimmed);
    // "abre el formulario y" promises more, whatever the punctuation says.
    let promises_more = ["y", "pero", "o", "que", "entonces", "porque", "and", "but"]
        .iter()
        .any(|word| trailing == *word || trailing.ends_with(&format!(" {word}")));
    if promises_more {
        evidence.push("it ends on a word that promises more".to_string());
    }
    let finished = sounds_finished && !promises_more;
    evidence.push(format!(
        "{} ms of silence{}",
        heard.silence_ms,
        if heard.final_by_recogniser {
            ", and the recogniser called the turn over"
        } else {
            ""
        }
    ));

    let scope = if finished && heard.silence_ms >= 300 {
        SilenceScope::EndOfTurn
    } else if heard.silence_ms >= 300 {
        SilenceScope::BetweenClauses
    } else {
        SilenceScope::WithinClause
    };

    ConversationalFloorState {
        current_speaker: Speaker::Nobody,
        speaker_hold: Probability::new(if promises_more { 0.8 } else { 0.1 }),
        thought_completion: Probability::new(if finished { 0.9 } else { 0.3 }),
        silence_scope: scope,
        silence_ms: heard.silence_ms,
        interruption_utility: Probability::ZERO,
        response_utility: Probability::new(addressee.probability),
        urgency: Probability::new(0.2),
        context_sufficiency: Probability::new(if heard.text.trim().is_empty() {
            0.0
        } else {
            0.8
        }),
        acknowledgement_only: Probability::new(if acknowledgement { 0.95 } else { 0.0 }),
        overlap: OverlapState::None,
        recent_interruption: InterruptionDirection::None,
        response_still_valid: Probability::ONE,
        evidence,
    }
}

/// The floor while Archie is speaking and the person says something over it.
///
/// `words` is what the recogniser has of it so far; an empty string is a voice with no
/// words yet, which is a bid for the floor until it turns out to be "mhm".
pub fn over_archie(words: &str) -> ConversationalFloorState {
    let acknowledgement = is_acknowledgement(words);
    ConversationalFloorState {
        current_speaker: Speaker::Archie,
        overlap: OverlapState::UserOverArchie,
        acknowledgement_only: Probability::new(if acknowledgement { 0.95 } else { 0.0 }),
        recent_interruption: InterruptionDirection::UserInterruptedArchie,
        response_still_valid: Probability::new(0.3),
        evidence: vec![if acknowledgement {
            format!("the person said «{words}» over Archie, an acknowledgement")
        } else {
            "the person started speaking over Archie".to_string()
        }],
        ..ConversationalFloorState::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archie_speech::{FloorAction, FloorPolicy, SpeechBudget};

    fn heard(text: &str) -> Utterance {
        Utterance {
            text: text.into(),
            final_by_recogniser: true,
            silence_ms: 700,
        }
    }

    #[test]
    fn words_are_compared_without_accents_case_or_punctuation() {
        assert_eq!(plain("¿Qué hora es, Archie?"), "que hora es archie");
        assert!(names_archie("Oye Archi, abre Firefox."));
        assert!(!names_archie("la arquitectura del edificio"));
        for misheard in [
            "Archier, qué ves?",
            "Oye Arshi.",
            "Harchi, mira esto",
            "oye ar chi qué es esto",
            "AC dibuja la Mona. Lisa.",
            "AC, abre Sentry",
        ] {
            assert!(names_archie(misheard), "{misheard}");
        }
        // The two letters are the name only at the head of a request.
        for other in ["el AC del auto", "AC.", "prende el ac"] {
            assert!(!names_archie(other), "{other}");
        }
        for other in [
            "Busca el archivo.",
            "el arco del puente",
            "voy a marchar",
            "es muy ancho",
            "the arch",
        ] {
            assert!(!names_archie(other), "{other}");
        }
        assert!(is_acknowledgement("Sí."));
        assert!(is_acknowledgement("ya po"));
        assert!(!is_acknowledgement("sí, ábrelo"));
    }

    #[test]
    fn a_request_that_names_archie_gets_an_answer() {
        let decision = FloorPolicy::default()
            .decide(&after_utterance(&heard("Oye Archie, ¿qué hora es?"), None));
        assert_eq!(decision.action, FloorAction::TakeFloor);
        assert_eq!(decision.budget, SpeechBudget::Full);
    }

    #[test]
    fn talk_in_the_room_that_is_not_to_archie_is_left_alone() {
        let state = after_utterance(&heard("¿Y tú fuiste al cumpleaños de la Cata?"), None);
        let decision = FloorPolicy::default().decide(&state);
        assert_eq!(decision.action, FloorAction::Silent);
        assert!(state
            .evidence
            .iter()
            .any(|line| line.contains("neither names")));
    }

    #[test]
    fn a_reply_right_after_archie_spoke_needs_no_name() {
        let answering = after_utterance(
            &heard("Sí, el de la izquierda."),
            Some(Duration::from_secs(3)),
        );
        assert_eq!(
            FloorPolicy::default().decide(&answering).action,
            FloorAction::TakeFloor
        );
        let too_late = after_utterance(
            &heard("Sí, el de la izquierda."),
            Some(Duration::from_secs(40)),
        );
        assert_eq!(
            FloorPolicy::default().decide(&too_late).action,
            FloorAction::Silent
        );
    }

    #[test]
    fn an_acknowledgement_neither_starts_nor_stops_archie() {
        let after = after_utterance(&heard("ya"), Some(Duration::from_secs(1)));
        assert_eq!(
            FloorPolicy::default().decide(&after).action,
            FloorAction::Silent
        );
        assert_eq!(
            FloorPolicy::default().decide(&over_archie("mhm")).action,
            FloorAction::ContinueSpeaking
        );
        assert_eq!(
            FloorPolicy::default()
                .decide(&over_archie("no, espera"))
                .action,
            FloorAction::Yield
        );
        assert_eq!(
            FloorPolicy::default().decide(&over_archie("")).action,
            FloorAction::Yield,
            "a voice with no words yet is a bid for the floor"
        );
    }

    #[test]
    fn a_sentence_left_hanging_is_not_the_end_of_the_turn() {
        let hanging = Utterance {
            text: "Archie, abre el formulario y".into(),
            final_by_recogniser: false,
            silence_ms: 450,
        };
        assert_eq!(
            FloorPolicy::default()
                .decide(&after_utterance(&hanging, None))
                .action,
            FloorAction::Silent
        );
    }
}

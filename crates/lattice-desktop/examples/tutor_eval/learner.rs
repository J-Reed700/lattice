//! The learner side of a session. A `say` turn is fixed; a `play` turn is written
//! by the model under test from the tutor's last reply, because the tutor's
//! question cannot be known in advance. The call goes straight to the server,
//! not through the recording proxy: it is the test's, not the app's.

use crate::course::Course;
use crate::Res;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Turn {
    Say {
        say: String,
    },
    Play {
        play: Play,
        #[serde(default)]
        misconception: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Play {
    Right,
    Wrong,
}

fn brief(course: &Course, play: Play, misconception: Option<&str>) -> String {
    let sources: Vec<String> = course
        .sources
        .iter()
        .map(|source| format!("# {}\n{}", source.title, source.text))
        .collect();
    let how = match (play, misconception) {
        (Play::Right, _) => {
            "Answer the tutor's question correctly and briefly, from the course text below.".into()
        }
        (Play::Wrong, Some(belief)) => {
            format!("Answer the tutor's question wrongly, holding this misconception: {belief}")
        }
        (Play::Wrong, None) => {
            "Answer the tutor's question wrongly, with a plausible mistake.".to_string()
        }
    };
    format!(
        "You are playing a learner in a tutoring chat, for a test of the tutor. {how} Write only \
         what the learner types: one to three plain sentences, first person, no citations. If \
         the tutor asked nothing, say in one short sentence that you are ready to go on.\n\n\
         The course text:\n\n{}",
        sources.join("\n\n")
    )
}

/// The learner's next message, answering `tutor_said`.
pub async fn played(
    upstream: &str,
    auth: Option<&(String, String)>,
    model: &str,
    course: &Course,
    tutor_said: &str,
    play: Play,
    misconception: Option<&str>,
) -> Res<String> {
    let mut request = reqwest::Client::new()
        .post(format!(
            "{}/v1/chat/completions",
            upstream.trim_end_matches('/')
        ))
        .json(&json!({
            "model": model,
            "messages": [
                {"role": "system", "content": brief(course, play, misconception)},
                {"role": "user", "content": format!("The tutor said:\n\n{tutor_said}")},
            ],
            "max_tokens": 300,
            "temperature": 0.7,
            "stream": false,
            "chat_template_kwargs": {"enable_thinking": false},
        }));
    if let Some((name, value)) = auth {
        request = request.header(name.as_str(), value.as_str());
    }
    let reply: Value = request.send().await?.error_for_status()?.json().await?;
    let text = reply
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or("the played learner wrote nothing")?;
    Ok(text.to_owned())
}

//! Notification text, in English and Romanian.
//!
//! Plain templates rather than the app's i18n package: this binary has no
//! webview, and pulling a JSON locale loader into a process that must start
//! in milliseconds and live in a few megabytes buys nothing. Both locales are
//! the same struct, so a missing string is a compile error, and a test
//! checks that every template carries the same placeholders in both.

#[derive(Debug)]
pub struct Strings {
    pub busy_title: &'static str,
    pub busy_cpu: &'static str,
    pub busy_memory: &'static str,
    pub hung_title: &'static str,
    pub hung_body: &'static str,
    pub started_from: &'static str,
    pub with_children: &'static str,
    pub also_busy: &'static str,
    pub end: &'static str,
    pub end_tree: &'static str,
    pub lower: &'static str,
    pub ignore: &'static str,
    pub ended: &'static str,
    pub ended_tree: &'static str,
    pub lowered: &'static str,
    pub already_gone: &'static str,
    pub failed: &'static str,
    pub ignored: &'static str,
    pub sound_test: &'static str,
}

pub const EN: Strings = Strings {
    busy_title: "{name} is freezing what you are using",
    busy_cpu: "Windows are slow to respond while it uses {cores} of {total} processor cores.",
    busy_memory: "Memory is full and the disk is standing in for it; it holds {size}.",
    hung_title: "{name} is not responding",
    hung_body: "Its window has not answered for {secs} seconds.",
    started_from: "Started from {chain}.",
    with_children: "Ending it also ends {count} processes it started.",
    also_busy: "Also busy: {list}.",
    end: "End it",
    end_tree: "End it and its {count}",
    lower: "Lower its priority",
    ignore: "Ignore 30 min",
    ended: "{name} was ended",
    ended_tree: "{name} and {count} processes it started were ended",
    lowered: "{name} now gives way to everything you are using",
    already_gone: "{name} had already exited",
    failed: "{name} could not be changed: {reason}",
    ignored: "{name} will not be mentioned for 30 minutes",
    sound_test: "This is the Vitals watchdog sound",
};

pub const RO: Strings = Strings {
    busy_title: "{name} blochează ce folosești",
    busy_cpu: "Ferestrele răspund greu cât timp folosește {cores} din {total} nuclee ale procesorului.",
    busy_memory: "Memoria e plină și discul ține locul ei; ocupă {size}.",
    hung_title: "{name} nu răspunde",
    hung_body: "Fereastra lui nu a mai răspuns de {secs} secunde.",
    started_from: "Pornit din {chain}.",
    with_children: "Închiderea lui oprește și {count} procese pornite de el.",
    also_busy: "Tot ocupate: {list}.",
    end: "Închide-l",
    end_tree: "Închide-l cu cele {count}",
    lower: "Scade-i prioritatea",
    ignore: "Ignoră 30 min",
    ended: "{name} a fost închis",
    ended_tree: "{name} și {count} procese pornite de el au fost închise",
    lowered: "{name} lasă acum loc la tot ce folosești",
    already_gone: "{name} se închisese deja",
    failed: "{name} nu a putut fi schimbat: {reason}",
    ignored: "{name} nu va mai fi semnalat 30 de minute",
    sound_test: "Acesta e sunetul watchdog-ului Vitals",
};

/// Fills `{name}`-style placeholders.
#[must_use]
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = template.to_owned();
    for (key, value) in values {
        out = out.replace(&format!("{{{key}}}"), value);
    }
    out
}

/// Escapes text for an XML attribute.
///
/// The toast crate escapes titles and body lines but interpolates a button's
/// label and argument into `<action content='…'/>` verbatim, so an image name
/// with an apostrophe (`Bob's tool.exe`) would break the XML and the whole
/// toast would silently fail to show.
#[must_use]
pub fn xml_attr(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\'' => out.push_str("&apos;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

/// Formats a byte count the way a person reads it.
#[must_use]
pub fn size(bytes: u64) -> String {
    let gib = bytes as f64 / f64::from(1_u32 << 30);
    if gib >= 1.0 {
        format!("{gib:.1} GB")
    } else {
        format!("{:.0} MB", bytes as f64 / f64::from(1_u32 << 20))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(s: &Strings) -> Vec<(&'static str, &'static str)> {
        vec![
            ("busy_title", s.busy_title),
            ("busy_cpu", s.busy_cpu),
            ("busy_memory", s.busy_memory),
            ("hung_title", s.hung_title),
            ("hung_body", s.hung_body),
            ("started_from", s.started_from),
            ("with_children", s.with_children),
            ("also_busy", s.also_busy),
            ("end", s.end),
            ("end_tree", s.end_tree),
            ("lower", s.lower),
            ("ignore", s.ignore),
            ("ended", s.ended),
            ("ended_tree", s.ended_tree),
            ("lowered", s.lowered),
            ("already_gone", s.already_gone),
            ("failed", s.failed),
            ("ignored", s.ignored),
            ("sound_test", s.sound_test),
        ]
    }

    fn placeholders(text: &str) -> Vec<&str> {
        let mut out: Vec<&str> = text
            .split('{')
            .skip(1)
            .filter_map(|rest| rest.split_once('}').map(|(name, _)| name))
            .collect();
        out.sort_unstable();
        out
    }

    #[test]
    fn both_locales_use_the_same_placeholders_in_every_string() {
        for ((field, en), (_, ro)) in fields(&EN).into_iter().zip(fields(&RO)) {
            assert_eq!(placeholders(en), placeholders(ro), "{field}");
            assert!(!ro.is_empty() && ro != en, "{field} is not translated");
        }
    }

    #[test]
    fn button_text_is_safe_inside_a_single_quoted_attribute() {
        assert_eq!(
            xml_attr("Bob's <tool> & \"x\""),
            "Bob&apos;s &lt;tool&gt; &amp; &quot;x&quot;"
        );
    }

    #[test]
    fn placeholders_are_filled_and_unknown_ones_left_visible() {
        assert_eq!(
            fill(EN.ended, &[("name", "java.exe")]),
            "java.exe was ended"
        );
        assert_eq!(fill("{a} {b}", &[("a", "1")]), "1 {b}");
    }

    #[test]
    fn sizes_read_as_gigabytes_or_megabytes() {
        assert_eq!(size(3 << 30), "3.0 GB");
        assert_eq!(size(512 << 20), "512 MB");
    }
}

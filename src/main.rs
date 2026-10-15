use std::{
    collections::HashMap,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute, queue,
    style::{Color, Print, Stylize},
    terminal::{self, ClearType},
};

const TIMED_EXERCISES: usize = 10;
const WORD_STAGE: usize = 20;
const SHOW_LAYOUT_THROUGH_STAGE: usize = 2;
const LAYOUTS_DIR: &str = "layouts";

#[derive(Clone, Debug, PartialEq, Eq)]
struct Layout {
    id: String,
    name: String,
    rows: Vec<String>,
    stages: Vec<String>,
    words_file: Option<String>,
    source: PathBuf,
}

impl Layout {
    fn keys_through(&self, stage: usize) -> String {
        let stage = stage.min(self.stages.len() - 1);
        let mut keys = String::new();
        for stage_keys in &self.stages[..=stage] {
            for key in stage_keys.chars() {
                if !keys.contains(key) {
                    keys.push(key);
                }
            }
        }
        keys
    }

    fn stage_count(&self) -> usize {
        self.stages.len()
    }
}

fn parse_layout_file(path: &Path) -> Result<Layout, String> {
    let content =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut id = None;
    let mut name = None;
    let mut rows = Vec::new();
    let mut stages = Vec::new();
    let mut stage_groups = None;
    let mut words_file = None;

    for (line_number, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("{}:{}: expected key=value", path.display(), line_number + 1))?;
        let key = key.trim();
        let value = value.trim();
        match key {
            "id" => id = Some(value.to_owned()),
            "name" => name = Some(value.to_owned()),
            "row" => rows.push(value.to_owned()),
            "stage" => stages.push(value.to_owned()),
            "stages" => stage_groups = Some(value.to_owned()),
            "words" => words_file = (!value.is_empty()).then(|| value.to_owned()),
            _ => {
                return Err(format!(
                    "{}:{}: unknown field '{key}'",
                    path.display(),
                    line_number + 1
                ));
            }
        }
    }

    let id = id.ok_or_else(|| format!("{}: missing id", path.display()))?;
    let name = name.ok_or_else(|| format!("{}: missing name", path.display()))?;
    if id.is_empty()
        || !id
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
    {
        return Err(format!(
            "{}: id must use lowercase ASCII, digits, or hyphens",
            path.display()
        ));
    }
    if rows.is_empty() || rows.iter().any(|row| row.is_empty()) {
        return Err(format!("{}: provide non-empty row fields", path.display()));
    }
    if let Some(groups) = stage_groups {
        if !stages.is_empty() {
            return Err(format!(
                "{}: use either stages= or repeated stage= fields, not both",
                path.display()
            ));
        }
        let groups: Vec<&str> = groups
            .split('|')
            .map(str::trim)
            .filter(|group| !group.is_empty())
            .collect();
        if groups.is_empty() || groups.len() > 50 {
            return Err(format!(
                "{}: stages= needs between 1 and 50 pipe-separated key groups",
                path.display()
            ));
        }
        stages = expand_stages(&groups);
    } else if stages.len() != 50 {
        return Err(format!(
            "{}: expected exactly 50 stage fields (or use stages= to expand a curriculum), found {}",
            path.display(),
            stages.len()
        ));
    }
    if stages.iter().any(|stage| stage.is_empty()) {
        return Err(format!(
            "{}: stage fields must not be empty",
            path.display()
        ));
    }
    if stages
        .iter()
        .any(|stage| stage.chars().any(char::is_whitespace))
    {
        return Err(format!(
            "{}: stage keys cannot contain whitespace",
            path.display()
        ));
    }
    let words = words_file
        .as_ref()
        .ok_or_else(|| format!("{}: missing words=<filename.csv>", path.display()))?;
    if Path::new(words).file_name().and_then(|part| part.to_str()) != Some(words.as_str()) {
        return Err(format!(
            "{}: words must be a filename in the layout directory",
            path.display()
        ));
    }

    Ok(Layout {
        id,
        name,
        rows,
        stages,
        words_file,
        source: path.to_path_buf(),
    })
}

fn expand_stages(groups: &[&str]) -> Vec<String> {
    let mut expanded: Vec<String> = groups.iter().map(|group| (*group).to_owned()).collect();
    let final_group = expanded.last().cloned().unwrap_or_default();
    expanded.resize(50, final_group);
    expanded
}

fn discover_layouts(directory: &Path) -> (Vec<Layout>, Vec<String>) {
    let mut layouts = Vec::new();
    let mut errors = Vec::new();
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => return (layouts, vec![format!("{}: {error}", directory.display())]),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("layout") {
            continue;
        }
        match parse_layout_file(&path) {
            Ok(layout) => layouts.push(layout),
            Err(error) => errors.push(error),
        }
    }
    layouts.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    (layouts, errors)
}

fn data_directory() -> PathBuf {
    if let Some(directory) = std::env::var_os("ALT_LAYOUTS_DATA_DIR") {
        return PathBuf::from(directory);
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cwd.join(LAYOUTS_DIR).is_dir() {
        return cwd;
    }
    let executable = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or(cwd);
    if executable.join(LAYOUTS_DIR).is_dir() {
        return executable;
    }
    executable
        .parent()
        .filter(|parent| parent.join(LAYOUTS_DIR).is_dir())
        .map(Path::to_path_buf)
        .unwrap_or(executable)
}

fn load_selected_layout(layouts: &[Layout], path: &Path) -> usize {
    std::fs::read_to_string(path)
        .ok()
        .as_deref()
        .map(str::trim)
        .and_then(|id| layouts.iter().position(|layout| layout.id == id))
        .unwrap_or(0)
}

fn load_words(layout: &Layout) -> Vec<String> {
    let Some(filename) = layout.words_file.as_deref() else {
        return Vec::new();
    };
    let parent = layout.source.parent().unwrap_or_else(|| Path::new("."));
    let path = parent.join(filename);
    std::fs::read_to_string(path)
        .map(|contents| parse_words(&contents))
        .unwrap_or_default()
}

fn parse_words(contents: &str) -> Vec<String> {
    contents
        .split(|ch: char| ch == ',' || ch.is_whitespace())
        .map(str::trim)
        .filter(|word| !word.is_empty() && word.chars().all(char::is_alphabetic))
        .map(str::to_lowercase)
        .collect()
}

#[derive(Debug)]
struct Trainer {
    layout: Layout,
    unlocked_stage: usize,
    mistakes: HashMap<char, u32>,
    seen: HashMap<char, u32>,
}

impl Trainer {
    fn new(layout: Layout) -> Self {
        Self {
            layout,
            unlocked_stage: 0,
            mistakes: HashMap::new(),
            seen: HashMap::new(),
        }
    }

    fn active_keys(&self) -> String {
        self.layout
            .keys_through(self.unlocked_stage)
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect()
    }

    fn record(&mut self, expected: char, actual: char) {
        *self.seen.entry(expected).or_default() += 1;
        if expected != actual {
            *self.mistakes.entry(expected).or_default() += 1;
        }
    }

    fn accuracy(&self) -> f64 {
        let total: u32 = self.seen.values().sum();
        if total == 0 {
            return 0.0;
        }
        (total - self.mistakes.values().sum::<u32>()) as f64 / total as f64
    }

    fn maybe_advance(&mut self) -> bool {
        if self.unlocked_stage + 1 >= self.layout.stage_count() {
            return false;
        }
        let keys = &self.layout.stages[self.unlocked_stage];
        let practiced: u32 = keys
            .chars()
            .map(|key| *self.seen.get(&key).unwrap_or(&0))
            .sum();
        let errors: u32 = keys
            .chars()
            .map(|key| *self.mistakes.get(&key).unwrap_or(&0))
            .sum();
        if practiced >= 12 && errors as f64 / practiced as f64 <= 0.15 {
            self.unlocked_stage += 1;
            true
        } else {
            false
        }
    }

    fn next_key(&self, offset: usize) -> char {
        let keys: Vec<char> = self.active_keys().chars().collect();
        let weights: Vec<u32> = keys
            .iter()
            .map(|key| 1 + self.mistakes.get(key).copied().unwrap_or(0).min(20))
            .collect();
        let total: u32 = weights.iter().sum();
        let mut pick = (offset.wrapping_mul(7).wrapping_add(3) as u32) % total;
        for (key, weight) in keys.into_iter().zip(weights) {
            if pick < weight {
                return key;
            }
            pick -= weight;
        }
        'a'
    }

    fn word_unlocked(&self) -> bool {
        self.unlocked_stage + 1 >= WORD_STAGE
    }

    fn show_layout_reference(&self) -> bool {
        self.unlocked_stage <= SHOW_LAYOUT_THROUGH_STAGE
    }

    fn reference_for_mode(&self, mode: PracticeMode) -> bool {
        mode == PracticeMode::Learning && self.show_layout_reference()
    }

    fn mode_available(&self, mode: PracticeMode) -> bool {
        mode == PracticeMode::Learning || self.word_unlocked()
    }

    fn next_exercise(&self, offset: usize) -> String {
        if self.word_unlocked() && offset % 3 == 2 {
            if let Some(word) = self.random_word(offset) {
                return word;
            }
        }
        let mut exercise = String::new();
        for index in 0..10 + offset % 5 {
            if index > 0 && index % 4 == 0 {
                exercise.push(' ');
            }
            exercise.push(self.next_key(offset * 17 + index));
        }
        exercise
    }

    fn random_word(&self, seed: usize) -> Option<String> {
        let keys = self.active_keys();
        let candidates: Vec<String> = load_words(&self.layout)
            .into_iter()
            .filter(|word| word.chars().all(|ch| keys.contains(ch)))
            .collect();
        if candidates.is_empty() {
            return None;
        }
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.subsec_nanos() as usize)
            .unwrap_or_default();
        Some(candidates[seed.wrapping_mul(7919).wrapping_add(nanos) % candidates.len()].clone())
    }

    fn practice_word(&self, seed: usize) -> String {
        self.random_word(seed)
            .unwrap_or_else(|| self.next_key(seed).to_string())
    }

    fn save_path(&self) -> PathBuf {
        progress_directory().join(format!("{}.tsv", self.layout.id))
    }

    fn save(&self) -> io::Result<()> {
        let mut contents = format!("stage\t{}\n", self.unlocked_stage);
        for (key, count) in &self.mistakes {
            contents.push_str(&format!("mistake\t{key}\t{count}\n"));
        }
        for (key, count) in &self.seen {
            contents.push_str(&format!("seen\t{key}\t{count}\n"));
        }
        std::fs::create_dir_all(progress_directory())?;
        std::fs::write(self.save_path(), contents)
    }

    fn reset(&mut self) {
        self.unlocked_stage = 0;
        self.mistakes.clear();
        self.seen.clear();
    }

    fn load(layout: Layout) -> Self {
        let mut trainer = Self::new(layout);
        let Ok(contents) = std::fs::read_to_string(trainer.save_path()) else {
            return trainer;
        };
        for line in contents.lines() {
            let fields: Vec<&str> = line.split('\t').collect();
            match fields.as_slice() {
                ["stage", stage] => trainer.unlocked_stage = stage.parse().unwrap_or(0),
                ["mistake", key, count] => {
                    if let (Some(key), Ok(count)) = (key.chars().next(), count.parse()) {
                        trainer.mistakes.insert(key, count);
                    }
                }
                ["seen", key, count] => {
                    if let (Some(key), Ok(count)) = (key.chars().next(), count.parse()) {
                        trainer.seen.insert(key, count);
                    }
                }
                _ => {}
            }
        }
        trainer.unlocked_stage = trainer.unlocked_stage.min(trainer.layout.stage_count() - 1);
        trainer
    }
}

fn progress_directory() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".altlayouts-trainer")
}

fn selected_layout_path() -> PathBuf {
    progress_directory().join("selected-layout")
}

struct RawMode;

impl RawMode {
    fn enable() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

fn main() -> io::Result<()> {
    let directory = data_directory().join(LAYOUTS_DIR);
    let (layouts, errors) = discover_layouts(&directory);
    if layouts.is_empty() {
        eprintln!("No valid layouts found in {}.", directory.display());
        for error in errors {
            eprintln!("  {error}");
        }
        eprintln!("See README.md for the layout descriptor format.");
        return Ok(());
    }
    for error in errors {
        eprintln!("Ignoring invalid layout: {error}");
    }
    let mut seen_ids = std::collections::HashSet::new();
    if layouts
        .iter()
        .any(|layout| !seen_ids.insert(layout.id.as_str()))
    {
        eprintln!("Layout IDs must be unique in {}.", directory.display());
        return Ok(());
    }

    let selection_path = selected_layout_path();
    let mut selected = load_selected_layout(&layouts, &selection_path);
    let mut trainer = Trainer::load(layouts[selected].clone());
    loop {
        execute!(
            io::stdout(),
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        )?;
        println!("{}", "AltLayouts Trainer".bold().with(Color::Cyan));
        println!(
            "Selected layout: {} ({}/{})",
            trainer.layout.name,
            selected + 1,
            layouts.len()
        );
        println!(
            "Stage {}/{}  Overall accuracy: {:.0}%",
            trainer.unlocked_stage + 1,
            trainer.layout.stage_count(),
            trainer.accuracy() * 100.0
        );
        println!("Active keys: {}", trainer.active_keys());
        println!();
        draw_layout_reference(&mut io::stdout(), &trainer, 5)?;
        println!(
            "[l] Next layout ({})  [r] Learning  [p] Practice words  [s] Stats  [x] Reset progress  [q] Quit",
            trainer.layout.name
        );
        io::stdout().flush()?;
        match read_char()?.to_ascii_lowercase() {
            'l' => {
                selected = (selected + 1) % layouts.len();
                std::fs::create_dir_all(progress_directory())?;
                std::fs::write(&selection_path, &layouts[selected].id)?;
                trainer = Trainer::load(layouts[selected].clone());
            }
            'r' => practice(&mut trainer, PracticeMode::Learning)?,
            'p' if trainer.mode_available(PracticeMode::Words) => {
                practice(&mut trainer, PracticeMode::Words)?
            }
            'p' => {
                println!(
                    "Word practice unlocks at stage {WORD_STAGE}. Keep learning to unlock it."
                );
                let _ = read_char()?;
            }
            's' => show_stats(&trainer)?,
            'x' => {
                if confirm_reset()? {
                    trainer.reset();
                    trainer.save()?;
                }
            }
            'q' | '\u{1b}' => break,
            _ => {}
        }
        trainer.save()?;
    }
    trainer.save()?;
    println!("\nProgress saved to {}", trainer.save_path().display());
    Ok(())
}

fn read_char() -> io::Result<char> {
    let _raw = RawMode::enable()?;
    loop {
        if let Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
        }) = event::read()?
        {
            return Ok(match code {
                KeyCode::Char(character) => character,
                KeyCode::Esc => '\u{1b}',
                _ => '\0',
            });
        }
    }
}

fn confirm_reset() -> io::Result<bool> {
    println!("\nReset this layout's saved progress? [y/N]");
    io::stdout().flush()?;
    Ok(matches!(read_char()?.to_ascii_lowercase(), 'y'))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PracticeMode {
    Learning,
    Words,
}

fn exercise_prompt(is_word: bool, target: &str) -> String {
    format!("{}:{target}", if is_word { "Word" } else { "Sequence" })
}

fn practice(trainer: &mut Trainer, mode: PracticeMode) -> io::Result<()> {
    let _raw = RawMode::enable()?;
    let mut stdout = io::stdout();
    let mut correct = 0;
    let mut errors = 0;
    let started = Instant::now();
    let mut stage_advanced = false;
    execute!(
        stdout,
        terminal::Clear(ClearType::All),
        cursor::MoveTo(0, 0)
    )?;
    if trainer.reference_for_mode(mode) {
        draw_layout_reference(&mut stdout, trainer, 2)?;
    }
    queue!(
        stdout,
        Print(format!(
            "{} mode. Enter submits; Backspace edits; Esc ends early.\r\n\r\n",
            match mode {
                PracticeMode::Learning => "Learning",
                PracticeMode::Words => "Word practice",
            }
        ))
    )?;
    stdout.flush()?;

    for step in 0..TIMED_EXERCISES {
        let expected = match mode {
            PracticeMode::Learning => trainer.next_exercise(step),
            PracticeMode::Words => trainer.practice_word(step),
        };
        let is_word = mode == PracticeMode::Words
            || (!expected.contains(' ')
                && expected.chars().count() < 10
                && trainer.word_unlocked());
        let show_reference = trainer.reference_for_mode(mode);
        let exercise_row = if show_reference { 11 } else { 4 };
        queue!(
            stdout,
            cursor::MoveTo(0, exercise_row),
            terminal::Clear(ClearType::FromCursorDown)
        )?;
        queue!(
            stdout,
            Print(format!("{}\r\n", exercise_prompt(is_word, &expected)).with(Color::Yellow))
        )?;
        let input_row = exercise_row + 1;
        queue!(stdout, cursor::MoveTo(0, input_row), Print("You: "))?;
        stdout.flush()?;

        let mut typed = Vec::new();
        let exercise_started = Instant::now();
        let aborted = loop {
            match event::read()? {
                Event::Key(KeyEvent {
                    code: KeyCode::Esc,
                    kind: KeyEventKind::Press,
                    ..
                }) => break true,
                Event::Key(KeyEvent {
                    code: KeyCode::Enter,
                    kind: KeyEventKind::Press,
                    ..
                }) => break false,
                Event::Key(KeyEvent {
                    code: KeyCode::Backspace,
                    kind: KeyEventKind::Press,
                    ..
                }) => {
                    if typed.pop().is_some() {
                        queue!(stdout, cursor::MoveLeft(1), Print(" "), cursor::MoveLeft(1))?;
                        stdout.flush()?;
                    }
                }
                Event::Key(KeyEvent {
                    code: KeyCode::Char(character),
                    kind: KeyEventKind::Press,
                    modifiers,
                    ..
                }) if !modifiers.contains(KeyModifiers::CONTROL) => {
                    let actual = if character.is_whitespace() {
                        ' '
                    } else {
                        character
                    };
                    if typed.len() < expected.chars().count() {
                        typed.push(actual);
                        let expected_char = expected.chars().nth(typed.len() - 1).unwrap();
                        let color = if expected_char == actual {
                            Color::Green
                        } else {
                            Color::Red
                        };
                        queue!(stdout, Print(actual.to_string().with(color)))?;
                        stdout.flush()?;
                    }
                }
                _ => {}
            }
        };
        if aborted {
            break;
        }

        let elapsed = exercise_started.elapsed().as_secs_f64();
        queue!(stdout, cursor::MoveTo(0, input_row + 1), Print("\r\n"))?;
        let mut compared = 0;
        for (index, expected_char) in expected.chars().enumerate() {
            if let Some(actual_char) = typed.get(index).copied() {
                trainer.record(expected_char, actual_char);
                compared += 1;
                if expected_char == actual_char {
                    correct += 1;
                } else {
                    errors += 1;
                }
            } else {
                trainer.record(expected_char, '\0');
                errors += 1;
            }
        }
        errors += typed.len().saturating_sub(compared);
        queue!(
            stdout,
            Print(if typed.iter().copied().eq(expected.chars()) {
                "✓".green()
            } else {
                "✗".red()
            })
        )?;
        if trainer.maybe_advance() {
            stage_advanced = true;
        }
        let wpm = if elapsed > 0.0 {
            typed.len() as f64 * 12.0 / elapsed
        } else {
            0.0
        };
        queue!(
            stdout,
            Print(format!(
                "  Time: {elapsed:.2}s  Speed: {wpm:.1} WPM\r\nSession: {correct}/{} correct characters, {errors} errors\r\n\r\n",
                correct + errors
            ))
        )?;
        stdout.flush()?;
    }

    drop(_raw);
    let elapsed = started.elapsed().as_secs_f64();
    println!("\nSession complete: {correct} correct, {errors} errors in {elapsed:.1}s.");
    if elapsed > 0.0 {
        println!("Average speed: {:.1} WPM", correct as f64 * 12.0 / elapsed);
    }
    if stage_advanced && trainer.unlocked_stage < trainer.layout.stage_count() {
        println!("New stage unlocked: {}", trainer.unlocked_stage + 1);
    }
    println!("Press any key to return.");
    io::stdout().flush()?;
    let _ = read_char()?;
    Ok(())
}

fn draw_layout_reference(
    stdout: &mut impl Write,
    trainer: &Trainer,
    start_row: u16,
) -> io::Result<()> {
    queue!(
        stdout,
        cursor::MoveTo(0, start_row),
        Print(format!(
            "{} layout reference (unlocked keys highlighted):\r\n",
            trainer.layout.name
        ))
    )?;
    for row in &trainer.layout.rows {
        for key in row.chars().filter(|character| !character.is_whitespace()) {
            let shown = key.to_string();
            if trainer.active_keys().contains(key) {
                queue!(stdout, Print(format!("[{shown}] ").with(Color::Cyan)))?;
            } else {
                queue!(stdout, Print(format!(" {shown}  ").dark_grey()))?;
            }
        }
        queue!(stdout, Print("\r\n"))?;
    }
    queue!(stdout, Print("\r\n"))?;
    stdout.flush()
}

fn show_stats(trainer: &Trainer) -> io::Result<()> {
    execute!(
        io::stdout(),
        terminal::Clear(ClearType::All),
        cursor::MoveTo(0, 0)
    )?;
    println!("AltLayouts Trainer — {} Stats", trainer.layout.name);
    println!(
        "Stage {}/{}",
        trainer.unlocked_stage + 1,
        trainer.layout.stage_count()
    );
    println!("Overall accuracy: {:.1}%", trainer.accuracy() * 100.0);
    let mut stats: Vec<(char, u32, u32)> = trainer
        .active_keys()
        .chars()
        .map(|key| {
            (
                key,
                *trainer.mistakes.get(&key).unwrap_or(&0),
                *trainer.seen.get(&key).unwrap_or(&0),
            )
        })
        .collect();
    stats.sort_by_key(|(_, errors, _)| std::cmp::Reverse(*errors));
    for (key, errors, seen) in stats {
        let accuracy = if seen == 0 {
            0.0
        } else {
            100.0 * (seen - errors) as f64 / seen as f64
        };
        println!("  {key}: {accuracy:5.1}% ({errors} misses / {seen} attempts)");
    }
    println!("\nPress any key to return.");
    io::stdout().flush()?;
    let _ = read_char()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_layout(stages: Vec<&str>) -> Layout {
        Layout {
            id: "test-layout".into(),
            name: "Test Layout".into(),
            rows: vec!["a b c".into(), "d e f".into()],
            stages: stages.into_iter().map(str::to_owned).collect(),
            words_file: None,
            source: PathBuf::from("layouts/test.layout"),
        }
    }

    #[test]
    fn descriptor_parser_accepts_unicode_and_exact_fifty_stages() {
        let mut text = String::from(
            "id=test-ru\nname=Russian Phonetic\nrow=й ц у к\nrow=ф ы в а\nwords=test-words.csv\n",
        );
        for index in 0..50 {
            let stage = if index == 0 { "фыва" } else { "й" };
            text.push_str(&format!("stage={stage}\n"));
        }
        let path =
            std::env::temp_dir().join(format!("altlayouts-test-{}.layout", std::process::id()));
        std::fs::write(&path, text).unwrap();
        let layout = parse_layout_file(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(layout.id, "test-ru");
        assert_eq!(layout.stages.len(), 50);
        assert!(layout.keys_through(0).contains('ф'));
        assert!(layout.keys_through(0).contains('ы'));
    }

    #[test]
    fn compact_descriptor_expands_fifty_stages_from_key_groups() {
        let path =
            std::env::temp_dir().join(format!("altlayouts-compact-{}.layout", std::process::id()));
        std::fs::write(
            &path,
            "id=compact\nname=Compact\nrow=a s d f\nwords=words.csv\nstages=asdf|j|k|l\n",
        )
        .unwrap();
        let layout = parse_layout_file(&path).unwrap();
        let _ = std::fs::remove_file(path);
        assert_eq!(layout.stage_count(), 50);
        assert_eq!(layout.stages[1], "j");
        assert_eq!(layout.stages[49], "l");
    }

    #[test]
    fn invalid_stage_count_is_rejected() {
        let path =
            std::env::temp_dir().join(format!("altlayouts-invalid-{}.layout", std::process::id()));
        std::fs::write(&path, "id=x\nname=X\nrow=a\nstage=a\n").unwrap();
        assert!(parse_layout_file(&path).unwrap_err().contains("exactly 50"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn selected_layout_falls_back_to_first_discovered() {
        let first = test_layout(vec!["a"; 50]);
        let mut second = test_layout(vec!["b"; 50]);
        second.id = "second-layout".into();
        let layouts = vec![first, second];
        let path = std::env::temp_dir().join(format!("altlayouts-selected-{}", std::process::id()));
        std::fs::write(&path, "missing").unwrap();
        assert_eq!(load_selected_layout(&layouts, &path), 0);
        std::fs::write(&path, "second-layout").unwrap();
        assert_eq!(load_selected_layout(&layouts, &path), 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn cyrillic_words_are_parsed_and_filtered_by_unlocked_keys() {
        assert_eq!(parse_words("мама, мир, hello1"), vec!["мама", "мир"]);
        let layout = test_layout(
            std::iter::once("м")
                .chain(std::iter::repeat_n("а", 49))
                .collect(),
        );
        assert!(layout.keys_through(0).contains('м'));
        assert!(layout.keys_through(1).contains('а'));
    }

    #[test]
    fn every_builtin_layout_descriptor_has_fifty_stages_and_loads() {
        let (layouts, errors) = discover_layouts(&PathBuf::from(LAYOUTS_DIR));
        assert!(errors.is_empty(), "{errors:?}");
        assert!(layouts.len() >= 5);
        assert!(layouts.iter().all(|layout| layout.stage_count() == 50));
        let ids: Vec<&str> = layouts.iter().map(|layout| layout.id.as_str()).collect();
        for id in [
            "colemak",
            "colemak-dh",
            "dvorak",
            "workman",
            "russian-phonetic",
        ] {
            assert!(ids.contains(&id), "missing layout {id}");
        }
        for layout in layouts {
            let words = load_words(&layout);
            assert!(!words.is_empty(), "{} dictionary should load", layout.id);
            let available: Vec<_> = words
                .iter()
                .filter(|word| word.chars().all(|ch| layout.keys_through(19).contains(ch)))
                .collect();
            assert!(
                !available.is_empty(),
                "{} should have stage-20 words",
                layout.id
            );
        }
    }

    #[test]
    fn compact_curriculum_expands_to_exactly_fifty_stages() {
        let expanded = expand_stages(&["as", "d", "f"]);
        assert_eq!(expanded.len(), 50);
        assert_eq!(expanded[1], "d");
    }
}

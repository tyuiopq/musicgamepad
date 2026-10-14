//! Профили на диске: встроенные + файлы пользователя в папке профилей.
//! Профиль пользователя с именем встроенного перекрывает встроенный
//! (так «Классику» можно менять и вернуть к заводской, удалив файл).

use std::path::PathBuf;

use engine::model::Profile;
use engine::presets;
use serde::Serialize;

#[derive(Debug, PartialEq, Serialize)]
pub struct ProfileEntry {
    pub name: String,
    /// Есть встроенный профиль с таким именем.
    pub builtin: bool,
    /// Встроенный профиль изменён пользователем (его можно вернуть к заводскому).
    pub modified: bool,
    /// Раздел библиотеки: builtin, daw, synth или mine.
    pub category: String,
    pub target: String,
    pub description: String,
}

pub struct ProfileStore {
    dir: PathBuf,
}

fn builtin() -> Vec<Profile> {
    presets::library()
}

pub fn validated(profile: &Profile) -> Result<(), String> {
    profile.validate().map_err(|e| e.to_string())?;
    if profile.name.trim().is_empty() {
        return Err("у профиля должно быть имя".into());
    }
    Ok(())
}

fn file_name_for(name: &str) -> String {
    let safe: String = name.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '_' } else { c }).collect();
    format!("{}.json", safe.trim())
}

impl ProfileStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Профили пользователя вместе с их файлами, по имени.
    fn user_files(&self) -> Vec<(PathBuf, Profile)> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return Vec::new() };
        let mut v: Vec<(PathBuf, Profile)> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .filter_map(|p| {
                let profile = Profile::from_json(&std::fs::read_to_string(&p).ok()?).ok()?;
                Some((p, profile))
            })
            .collect();
        v.sort_by(|a, b| a.1.name.cmp(&b.1.name));
        v
    }

    pub fn list(&self) -> Vec<ProfileEntry> {
        let user: Vec<Profile> = self.user_files().into_iter().map(|(_, p)| p).collect();
        let builtin = builtin();
        let entry = |p: &Profile, builtin: bool, modified: bool| ProfileEntry {
            name: p.name.clone(),
            builtin,
            modified,
            category: if builtin { p.meta.category.clone() } else { "mine".into() },
            target: p.meta.target.clone(),
            description: p.meta.description.clone(),
        };
        let mut v: Vec<ProfileEntry> =
            builtin.iter().map(|b| entry(b, true, user.iter().any(|u| u.name == b.name))).collect();
        v.extend(user.iter().filter(|u| !builtin.iter().any(|b| b.name == u.name)).map(|p| entry(p, false, false)));
        v
    }

    pub fn find(&self, name: &str) -> Option<Profile> {
        self.user_files().into_iter().map(|(_, p)| p).chain(builtin()).find(|p| p.name == name)
    }

    /// Сохраняет профиль. `previous` — имя, под которым он был сохранён (None — новый):
    /// при переименовании старый файл пользователя удаляется, встроенный остаётся.
    pub fn save(&self, profile: &Profile, previous: Option<&str>) -> Result<(), String> {
        validated(profile)?;
        let files = self.user_files();
        // Новое имя (переименование или ещё не сохранённая копия) не должно затирать другой профиль
        let is_new_name = previous != Some(profile.name.as_str());
        if is_new_name && self.find(&profile.name).is_some() {
            return Err(format!("профиль «{}» уже есть — выбери другое имя", profile.name));
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let path = files
            .iter()
            .find(|(_, p)| p.name == profile.name)
            .map(|(path, _)| path.clone())
            .unwrap_or_else(|| self.dir.join(file_name_for(&profile.name)));
        std::fs::write(&path, profile.to_json()).map_err(|e| e.to_string())?;
        if let Some(old) = previous.filter(|_| is_new_name)
            && let Some((old_path, _)) = files.iter().find(|(_, p)| p.name == old)
            && *old_path != path
        {
            let _ = std::fs::remove_file(old_path);
        }
        Ok(())
    }

    /// Свободное имя для импортируемого профиля: «Имя», «Имя (2)», «Имя (3)»…
    pub fn free_name(&self, name: &str) -> String {
        let base = if name.trim().is_empty() { "Профиль" } else { name.trim() };
        if self.find(base).is_none() {
            return base.to_string();
        }
        (2..).map(|i| format!("{base} ({i})")).find(|n| self.find(n).is_none()).expect("бесконечный ряд")
    }

    /// Удаляет файл пользователя (для изменённого встроенного это «вернуть заводской»).
    pub fn delete(&self, name: &str) -> Result<(), String> {
        let (path, _) = self
            .user_files()
            .into_iter()
            .find(|(_, p)| p.name == name)
            .ok_or(format!("профиль «{name}» нельзя удалить"))?;
        std::fs::remove_file(path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(test: &str) -> (ProfileStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("musicgamepad-test-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        (ProfileStore::new(&dir), dir)
    }

    fn named(name: &str) -> Profile {
        let mut p = presets::classic();
        p.name = name.into();
        p
    }

    #[test]
    fn builtin_can_be_modified_and_reset() {
        let (s, dir) = store("reset");
        let mut p = presets::classic();
        p.music.root = 9;
        s.save(&p, Some("Классика")).unwrap();
        let classic = s.list().into_iter().find(|e| e.name == "Классика").unwrap();
        assert!(classic.builtin && classic.modified);
        assert_eq!(s.find("Классика").unwrap().music.root, 9);
        s.delete("Классика").unwrap();
        assert_eq!(s.find("Классика").unwrap().music.root, 0, "вернулся заводской");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rename_moves_file_and_copy_keeps_original() {
        let (s, dir) = store("rename");
        s.save(&named("Мой"), None).unwrap();
        s.save(&named("Мой новый"), Some("Мой")).unwrap();
        assert!(s.find("Мой").is_none());
        assert!(s.find("Мой новый").is_some());
        // копия (previous = None) не трогает оригинал
        s.save(&named("Мой новый (копия)"), None).unwrap();
        assert!(s.find("Мой новый").is_some());
        // переименование встроенного не удаляет встроенный
        s.save(&named("Своя классика"), Some("Классика")).unwrap();
        assert!(s.find("Классика").is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn new_name_never_overwrites_another_profile() {
        let (s, dir) = store("collide");
        s.save(&named("А"), None).unwrap();
        s.save(&named("Б"), None).unwrap();
        assert!(s.save(&named("А"), Some("Б")).is_err(), "переименование в занятое имя");
        assert!(s.save(&named("А"), None).is_err(), "копия с занятым именем");
        assert!(s.save(&named("Классика"), Some("А")).is_err(), "имя встроенного занято");
        assert!(s.find("Б").is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn free_name_for_import() {
        let (s, dir) = store("free");
        assert_eq!(s.free_name("Новый"), "Новый");
        assert_eq!(s.free_name("Классика"), "Классика (2)");
        s.save(&named("Классика (2)"), None).unwrap();
        assert_eq!(s.free_name("Классика"), "Классика (3)");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_empty_name_and_odd_characters_are_safe() {
        let (s, dir) = store("names");
        assert!(s.save(&named("  "), None).is_err());
        s.save(&named("a/b:c?"), None).unwrap();
        assert!(s.find("a/b:c?").is_some());
        let _ = std::fs::remove_dir_all(dir);
    }
}

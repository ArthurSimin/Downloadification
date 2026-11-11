//! Builds the Task Scheduler XML used on Windows.
//!
//! This module is intentionally platform independent so the generated XML can
//! be unit tested on Linux/macOS — important because the Windows installer is
//! otherwise hard to verify without a Windows machine.

/// Escape a value for use inside an XML text node or attribute.
pub fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Build the Scheduled Task definition.
///
/// The task runs the daemon at logon, never times out, restarts on failure and
/// ignores duplicate starts. It uses the interactive user token, so it needs
/// no stored password and no elevation.
pub fn task_xml(bin: &str, user: &str, task_name: &str) -> String {
    let user = xml_escape(user);
    let command = xml_escape(bin);
    let task = xml_escape(task_name);
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Automatically organizes the Downloads folder.</Description>
    <URI>\{task}</URI>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
    <RestartOnFailure>
      <Interval>PT1M</Interval>
      <Count>999</Count>
    </RestartOnFailure>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{command}</Command>
      <Arguments>daemon</Arguments>
    </Exec>
  </Actions>
</Task>
"#,
        task = task,
        user = user,
        command = command
    )
}

/// `schtasks.exe` requires the XML file to be UTF-16LE with a BOM.
pub fn encode_utf16le(contents: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(2 + contents.len() * 2);
    bytes.extend_from_slice(&[0xFF, 0xFE]);
    for unit in contents.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> String {
        task_xml(
            r"C:\Program Files\Downloadification\downloadification.exe",
            r"DESKTOP-1234\Arthur",
            "Downloadification",
        )
    }

    #[test]
    fn declaration_is_utf16() {
        assert!(sample().starts_with(r#"<?xml version="1.0" encoding="UTF-16"?>"#));
    }

    #[test]
    fn xml_is_well_formed() {
        let xml = sample();
        // roxmltree parses `&str` as UTF-8; the body is what matters here.
        let body = xml.split_once("?>").map(|(_, rest)| rest).unwrap_or(&xml);
        let doc = roxmltree::Document::parse(body).expect("task XML must be well formed");
        let names: Vec<&str> = doc.descendants().map(|n| n.tag_name().name()).collect();
        for required in [
            "Task",
            "LogonTrigger",
            "Principal",
            "Exec",
            "Command",
            "Arguments",
        ] {
            assert!(names.contains(&required), "missing <{required}>");
        }
        // The daemon must be the argument, not the command.
        let args = doc
            .descendants()
            .find(|n| n.tag_name().name() == "Arguments")
            .and_then(|n| n.text())
            .unwrap_or_default();
        assert_eq!(args.trim(), "daemon");
    }

    #[test]
    fn escapes_special_characters() {
        let xml = task_xml(r"C:\A&B\<x>.exe", "DOMAIN\\user", "Task");
        assert!(xml.contains("A&amp;B"));
        assert!(xml.contains("&lt;x&gt;"));
        // The escaped body must still parse.
        let body = xml.split_once("?>").map(|(_, rest)| rest).unwrap_or(&xml);
        roxmltree::Document::parse(body).expect("escaped XML must be well formed");
    }

    #[test]
    fn utf16_encoding_has_bom_and_round_trips() {
        let text = "hello \u{1f600}";
        let bytes = encode_utf16le(text);
        assert_eq!(&bytes[..2], &[0xFF, 0xFE]);
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        assert_eq!(String::from_utf16(&units).unwrap(), text);
    }

    #[test]
    fn uses_interactive_token_without_elevation() {
        let xml = sample();
        assert!(xml.contains("<LogonType>InteractiveToken</LogonType>"));
        assert!(xml.contains("<RunLevel>LeastPrivilege</RunLevel>"));
        assert!(xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
        assert!(xml.contains("<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>"));
    }
}

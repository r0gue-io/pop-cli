// use std::process::exit;

// use enum_str::AsStr;

// /// This file handlers logic that would execute when running a network runner
// enum_str! {
//     Zombienet,
//     // TODO : zombienet-linux-arm64 is unhandled
//     // (LinuxArm, "zombienet-linux-arm64"),
//     (Linux, "zombienet-linux-x64"),
//     (Mac, "zombienet-macos"),
// }

// pub fn runner(r: Run) -> Result<()> {
//     // TODO: FETCH these from .env
//     let zombienet_v = "v1.3.68";
//     let polkadot_v = "v1.1.0";
//     let os = match std::env::consts::OS {
//         "linux" => Zombienet::Linux,
//         "macos" => Zombienet::Mac,
//         x => {
//             eprintln!("{x} is an unsupported operating system");
//             exit(1);
//         }
//     };
//     if userland_zombinet(zombienet_v, &os) {
//         // Use that instead
//     } else {
//     }
//     Ok(())
// }

// /// Check if user already is using a version of zombinet at zombienet_v or later
// pub fn userland_zombinet(s: &str, os: &Zombienet) -> bool {
//     use semver::Version;
//     // trim `v` from front of s
//     let version = Version::parse(s.trim_start_matches('v')).unwrap();
//     // Run command `zombienet --version`
//     let user_zombienet_v = Version::parse(
//         Command::new(os.as_str())
//             .arg("version")
//             .output()
//             .expect("failed to execute process"),
//     )
//     .unwrap();
//     user_zombienet_v >= version
// }

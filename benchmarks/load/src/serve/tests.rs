use super::*;

fn parse(args: &[&str]) -> Result<Options, ServeError> {
    Options::parse(args.iter().map(|arg| (*arg).to_owned()))
}

#[test]
fn cpus_flag_is_a_comma_list() {
    let options = parse(&["--workers", "2", "--cpus", "0,2"]).expect("valid");

    assert_eq!(options.cpus, Some(vec![0, 2]));
    assert!(parse(&["--cpus", "a"]).is_err());
}

#[test]
fn cpus_flag_expands_ranges() {
    let options = parse(&["--cpus", "0-2,5"]).expect("valid");

    assert_eq!(options.cpus, Some(vec![0, 1, 2, 5]));
    assert!(parse(&["--cpus", "3-1"]).is_err());
    assert_eq!(parse(&[]).expect("valid").cpus, None);
}

#[test]
fn cpu_ids_beyond_the_cpu_set_are_rejected() {
    assert!(matches!(
        parse(&["--cpus", "1024"]),
        Err(ServeError::CpuOutOfRange(1024))
    ));
    assert!(matches!(
        parse(&["--cpus", "1000-2000"]),
        Err(ServeError::CpuOutOfRange(2000))
    ));
    assert!(parse(&["--cpus", "1023"]).is_ok());
}

#[cfg(target_os = "linux")]
#[test]
fn pinning_to_a_missing_cpu_fails_loudly() {
    assert!(matches!(
        check_pinning(&[1023]),
        Err(ServeError::Pin { .. })
    ));
    assert!(check_pinning(&[0]).is_ok());
}

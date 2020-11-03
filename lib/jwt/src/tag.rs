use std::iter::FromIterator;
use std::str::FromStr;

use itertools::Itertools;

pub fn parse_multiple<
    T: FromStr + Sized,
    S: AsRef<str>,
    I: Iterator<Item = S>,
    C: FromIterator<T>,
>(
    iter: I,
) -> Result<C, T::Err> {
    iter.map(|x| x.as_ref().parse()).collect()
}

pub fn parse_space_delimited<T: FromStr + Sized, C: FromIterator<T>>(
    source: &str,
) -> Result<C, T::Err> {
    parse_multiple(source.split(' ').filter(|x| !x.is_empty()))
}

pub fn serialize_space_delimited<'a, S: AsRef<str> + 'static, I: Iterator<Item = &'a S>>(
    iter: I,
) -> String {
    iter.map(|x| -> &str { x.as_ref() }).join(" ")
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use strum_macros::{AsRefStr, EnumString};

    use super::*;

    #[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, AsRefStr, EnumString)]
    #[strum(serialize_all = "snake_case")]
    pub enum Scope {
        Superuser,
        OfflineAccess,
    }

    #[test]
    fn test_parse_space_delimited() -> Result<(), Box<dyn std::error::Error>> {
        let test: HashSet<Scope> = parse_space_delimited("superuser offline_access")?;
        let test2: Vec<Scope> = parse_space_delimited("superuser")?;

        assert_eq!(test.len(), 2);
        assert_eq!(test2.len(), 1);
        assert!(test.contains(&Scope::Superuser));
        assert!(test.contains(&Scope::OfflineAccess));
        assert_eq!(test2[0], Scope::Superuser);
        Ok(())
    }

    #[test]
    fn test_error() -> Result<(), Box<dyn std::error::Error>> {
        let test: Result<HashSet<Scope>, _> = parse_space_delimited("superuser illegal_variant");
        assert!(test.is_err());
        Ok(())
    }

    #[test]
    fn test_serialize_space_delimited() -> Result<(), Box<dyn std::error::Error>> {
        let test = vec![Scope::Superuser, Scope::OfflineAccess];
        let ser = serialize_space_delimited(test.iter());
        assert_eq!(ser, "superuser offline_access");
        Ok(())
    }
}

use crate::OaxacaError;

#[derive(Debug, Clone)]
pub struct Formula {
    pub outcome: String,
    pub predictors: Vec<String>,
    pub categorical_predictors: Vec<String>,
}

impl Formula {
    /// Parses an R-style formula string (e.g., "wage ~ education + experience + C(sector)").
    pub fn parse(formula_str: &str) -> Result<Formula, OaxacaError> {
        let parts: Vec<&str> = formula_str.split('~').collect();
        if parts.len() != 2 {
            return Err(OaxacaError::InvalidGroupVariable(format!(
                "Invalid formula format. Expected 'outcome ~ predictors', got '{}'",
                formula_str
            )));
        }

        let outcome = parts[0].trim().to_string();
        if outcome.is_empty() {
            return Err(OaxacaError::InvalidGroupVariable(
                "Outcome variable is missing".to_string(),
            ));
        }

        let predictors_part = parts[1];
        let mut predictors = Vec::new();
        let mut categorical_predictors = Vec::new();

        for term in predictors_part.split('+') {
            let term = term.trim();
            if term.is_empty() {
                continue;
            }

            if term.starts_with("C(") && term.ends_with(")") {
                let var_name = &term[2..term.len() - 1];
                categorical_predictors.push(var_name.trim().to_string());
            } else if term.starts_with("factor(") && term.ends_with(")") {
                let var_name = &term[7..term.len() - 1];
                categorical_predictors.push(var_name.trim().to_string());
            } else {
                predictors.push(term.to_string());
            }
        }

        if predictors.is_empty() && categorical_predictors.is_empty() {
            return Err(OaxacaError::InvalidGroupVariable(
                "No predictors specified".to_string(),
            ));
        }

        Ok(Formula {
            outcome,
            predictors,
            categorical_predictors,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let f = Formula::parse("wage ~ education + experience")
            .expect("Failed to parse simple formula");
        assert_eq!(f.outcome, "wage");
        assert_eq!(f.predictors, vec!["education", "experience"]);
        assert!(f.categorical_predictors.is_empty());
    }

    #[test]
    fn test_parse_categorical() {
        let f = Formula::parse("wage ~ education + C(sector) + factor(gender)")
            .expect("Failed to parse categorical formula");
        assert_eq!(f.outcome, "wage");
        assert_eq!(f.predictors, vec!["education"]);
        assert_eq!(f.categorical_predictors, vec!["sector", "gender"]);
    }

    #[test]
    fn test_parse_whitespace() {
        let f = Formula::parse("  wage   ~   education  +  experience  ")
            .expect("Failed to parse formula with extra whitespace");
        assert_eq!(f.outcome, "wage");
        assert_eq!(f.predictors, vec!["education", "experience"]);
    }
}

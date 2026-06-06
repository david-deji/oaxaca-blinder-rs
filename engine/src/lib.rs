pub mod analysis;
pub mod defensibility;
pub mod types;
mod verification_test;
mod ab_binding_regression_test;

#[cfg(feature = "wasm")]
use crate::analysis::{decompose_inner, optimize_inner};
#[cfg(feature = "wasm")]
use crate::types::*;
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

// --- Wasm Wrappers ---

// --- Wasm Wrappers ---

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn init_panic_hook() {
    console_error_panic_hook::set_once();
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn decompose(val: JsValue) -> Result<JsValue, JsValue> {
    let req: DecompositionRequest = serde_wasm_bindgen::from_value(val)?;
    let res = decompose_inner(req).map_err(|e| JsValue::from_str(&e))?;
    Ok(serde_wasm_bindgen::to_value(&res)?)
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn optimize(val: JsValue) -> Result<JsValue, JsValue> {
    let req: OptimizationRequest = serde_wasm_bindgen::from_value(val)?;
    let res = optimize_inner(req).map_err(|e| JsValue::from_str(&e))?;
    Ok(serde_wasm_bindgen::to_value(&res)?)
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn verify_adjustments(val: JsValue) -> Result<JsValue, JsValue> {
    let req: VerificationRequest = serde_wasm_bindgen::from_value(val)?;
    let res = crate::analysis::verify_inner(req).map_err(|e| JsValue::from_str(&e))?;
    Ok(serde_wasm_bindgen::to_value(&res)?)
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn calculate_efficient_frontier(val: JsValue) -> Result<JsValue, JsValue> {
    let req: EfficientFrontierRequest = serde_wasm_bindgen::from_value(val)?;
    let res = crate::analysis::calculate_efficient_frontier_inner(req)
        .map_err(|e| JsValue::from_str(e.as_str()))?;
    Ok(serde_wasm_bindgen::to_value(&res)?)
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn check_defensibility(val: JsValue) -> Result<JsValue, JsValue> {
    let req: VerificationRequest = serde_wasm_bindgen::from_value(val)?;
    let res = crate::defensibility::check_defensibility_inner(req)
        .map_err(|e| JsValue::from_str(e.as_str()))?;
    Ok(serde_wasm_bindgen::to_value(&res)?)
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub async fn validate_access_code(code: String, registry_url: String) -> Result<JsValue, JsValue> {
    let res = crate::access::validate_access_code_inner(&code, &registry_url)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    Ok(serde_wasm_bindgen::to_value(&res)?)
}

mod access;

#[cfg(test)]
#[cfg(feature = "wasm")]
mod tests {
    use super::*;
    use crate::types::{DecompositionRequest, EfficientFrontierRequest};
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn test_calculate_efficient_frontier_valid_structure() {
        let csv_data = b"wage,gender,education\n50000,Male,12\n40000,Female,12\n60000,Male,14\n45000,Female,14\n70000,Male,16\n55000,Female,16\n50000,Male,12\n40000,Female,12\n60000,Male,14\n45000,Female,14\n70000,Male,16\n55000,Female,16\n50000,Male,12\n40000,Female,12\n60000,Male,14\n45000,Female,14\n70000,Male,16\n55000,Female,16\n50000,Male,12\n40000,Female,12\n60000,Male,14\n45000,Female,14\n70000,Male,16\n55000,Female,16".to_vec();
        let req = EfficientFrontierRequest {
            decomposition_params: DecompositionRequest {
                csv_data,
                outcome_variable: "wage".to_string(),
                group_variable: "gender".to_string(),
                reference_group: "Male".to_string(),
                predictors: vec!["education".to_string()],
                categorical_predictors: None,
                three_fold: None,
                quantile: None,
                reference_coefficients: None,
                bootstrap_reps: None,
            },
            steps: Some(10),
            max_budget: Some(10000.0),
        };

        let js_val = serde_wasm_bindgen::to_value(&req).unwrap();

        let result = calculate_efficient_frontier(js_val);

        match result {
            Ok(val) => {
                assert!(val.is_object());
            }
            Err(e) => panic!("Expected Ok, got Err: {:?}", e),
        }
    }

    #[wasm_bindgen_test]
    fn test_calculate_efficient_frontier_invalid_js_value() {
        let js_val = JsValue::from_str("not an object");
        let result = calculate_efficient_frontier(js_val);

        assert!(result.is_err());
        let err = result.unwrap_err();
        if let Some(js_str) = err.as_string() {
            assert!(!js_str.is_empty());
        } else {
            assert!(err.is_object() || err.is_string());
        }
    }
}

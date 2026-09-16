// Test modules for infrastructure services

// Vertical-slice migration (qa): test lives in features/qa/tests/.
#[path = "../../../features/qa/tests/conversational_service.rs"]
pub mod test_conversational_qa_service;
pub mod test_model_manager;

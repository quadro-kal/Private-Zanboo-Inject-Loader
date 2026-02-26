#![no_std]

// ---------------------------------------------------------
// ZIL_CORE: MASTER BRAIN HUB
// File ini menghubungkan semua folder terpisah menjadi satu crate.
// ---------------------------------------------------------

pub mod evolution {
    #[path = "../../evolution/src/chip_detector.rs"]
    pub mod chip_detector;

    #[path = "../../evolution/src/cs_bypasser.rs"] 
    pub mod cs_bypasser;
    
    #[path = "../../evolution/src/heuristic_scanner.rs"] 
    pub mod heuristic_scanner;
    
    #[path = "../../evolution/src/kcall_primitive.rs"] 
    pub mod kcall_primitive;
    
    #[path = "../../evolution/src/offset_calculator.rs"] 
    pub mod offset_calculator;
    
    #[path = "../../evolution/src/payload_escalation.rs"] 
    pub mod payload_escalation;
}

pub mod healing {
    #[path = "../../healing/src/engine.rs"] 
    pub mod engine;
    
    #[path = "../../healing/src/stats.rs"] 
    pub mod stats;
    
    #[path = "../../healing/src/state.rs"] 
    pub mod state;
}

pub mod memory {
    #[path = "../../memory/src/scanner.rs"] 
    pub mod scanner;
}

pub mod npu {
    #[path = "../../npu/src/engine.rs"]
    pub mod engine;

    #[path = "../../npu/src/model_loader.rs"]
    pub mod model_loader;

    // SARAN 3: FFI bridge ke ane_asymmetric.c
    #[path = "../../npu/src/npu_asymmetric.rs"]
    pub mod npu_asymmetric;
}

pub mod drivers {
    pub mod npu {
        #[path = "../../../driver/npu/accelerator.rs"] 
        pub mod accelerator;
        
        pub use accelerator::HardwareAccelerator;
    }
}

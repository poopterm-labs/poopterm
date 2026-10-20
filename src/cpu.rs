//! Runtime CPU feature detection for the hot paths compiled twice: once for
//! baseline x86-64 and once for x86-64-v3 (AVX2, BMI1/2, LZCNT, FMA).

/// Whether the CPU runs the x86-64-v3 variants. The standard library caches
/// the detection, so this is a load and a test per call.
#[inline(always)]
pub(crate) fn has_v3() -> bool {
	#[cfg(target_arch = "x86_64")]
	{
		use std::arch::is_x86_feature_detected as has;
		has!("avx2") && has!("bmi1") && has!("bmi2") && has!("lzcnt") && has!("fma") && has!("popcnt")
	}
	#[cfg(not(target_arch = "x86_64"))]
	false
}

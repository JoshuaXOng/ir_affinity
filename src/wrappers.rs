#[cfg(target_os = "windows")]
pub use windows::*;

#[cfg(target_os = "windows")]
mod windows {
    use std::fmt::{Debug, Formatter, Error};

    use windows::Win32::System::SystemInformation::{SYSTEM_CPU_SET_INFORMATION, SYSTEM_CPU_SET_INFORMATION_0_0};

    pub struct SystemCpuSetInformation(SYSTEM_CPU_SET_INFORMATION);

    impl SystemCpuSetInformation {
        pub fn new(inner: SYSTEM_CPU_SET_INFORMATION) -> Self {
            Self(inner)
        }

        pub fn get(&self) -> &SYSTEM_CPU_SET_INFORMATION_0_0 {
            unsafe {
                &self.0.Anonymous.CpuSet
            }
        }

        pub fn get_id(&self) -> u32 {
            self.get().Id
        }

        pub fn get_logical(&self) -> u8 {
            self.get().LogicalProcessorIndex
        }

        pub fn get_physical(&self) -> u8 {
            self.get().CoreIndex
        }
    }

    impl Debug for SystemCpuSetInformation {
        fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
            f.debug_struct("SystemCpuSetInformation")
                .field("id", &self.get().Id)
                .field("logical", &self.get().LogicalProcessorIndex)
                .field("physical", &self.get().CoreIndex)
                .finish()
        }
    }
}

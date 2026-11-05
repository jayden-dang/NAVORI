#![allow(dead_code)]

use navori_kernel::Kernel;
use navori_store::Store;

pub fn kernel(store: Store) -> Kernel {
    Kernel::builder(store).start()
}

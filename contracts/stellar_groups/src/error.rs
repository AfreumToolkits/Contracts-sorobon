use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    GroupNotFound = 2,
    GroupFull = 3,
    AlreadyMember = 4,
    NotMember = 5,
    AlreadyContributed = 6,
    RoundNotReady = 7,
    GroupInactive = 8,
    InvalidParams = 9,
}

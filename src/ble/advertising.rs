const MAXIMUM_ADVERTISE_LENGTH: usize = 31;

pub struct AdvertisingData {
    data: [u8; MAXIMUM_ADVERTISE_LENGTH],
    used_bytes: usize,
}

impl AdvertisingData {
    pub const fn new() -> Self {
        Self {
            data: [0; MAXIMUM_ADVERTISE_LENGTH],
            used_bytes: 0,
        }
    }

    const fn add_internal(mut self, element_type: u8, element_data: &[u8]) -> Self {
        self.data[self.used_bytes] = (element_data.len() + 1) as u8;
        self.data[self.used_bytes + 1] = element_type;

        if self.used_bytes + element_data.len() > MAXIMUM_ADVERTISE_LENGTH {
            panic!("Advertising data is too big. Try shortening the keyboard name.");
        }

        let mut offset = 0;

        while offset < element_data.len() {
            self.data[self.used_bytes + 2 + offset] = element_data[offset];
            offset += 1;
        }

        self.used_bytes += 2 + offset;

        self
    }

    pub const fn add_flags(self, flags: u8) -> Self {
        self.add_internal(0x1, &[flags])
    }

    pub const fn add_services(self, services: &[u8]) -> Self {
        self.add_internal(0x3, services)
    }

    pub const fn add_name(self, name: &[u8]) -> Self {
        self.add_internal(0x9, name)
    }

    pub fn get_slice(&self) -> &[u8] {
        &self.data[..self.used_bytes]
    }
}

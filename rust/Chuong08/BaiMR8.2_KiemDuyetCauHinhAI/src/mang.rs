//! Mạng IPv4 dạng "a.b.c.d/n" (tương đương ipaddress.ip_network(strict=True) của Python, rút gọn).

use std::fmt;
use std::net::Ipv4Addr;

#[derive(Clone, Copy)]
pub struct Net {
    pub addr: u32, // địa chỉ mạng
    pub plen: u32, // độ dài tiền tố
}

impl Net {
    /// "10.20.1.0/25" hoặc "10.20.1.0/255.255.255.128"; None nếu sai cú pháp hoặc còn bit máy.
    pub fn parse(text: &str) -> Option<Net> {
        let (ip, p) = text.split_once('/').unwrap_or((text, "32"));
        let a = u32::from(ip.parse::<Ipv4Addr>().ok()?);
        let plen = match p.parse::<u32>() {
            Ok(n) if n <= 32 => n,
            Ok(_) => return None,
            Err(_) => {
                let m = u32::from(p.parse::<Ipv4Addr>().ok()?);
                let n = m.leading_ones();
                if m.checked_shl(n).unwrap_or(0) != 0 {
                    return None; // mặt nạ không liên tục
                }
                n
            }
        };
        let net = Net { addr: a, plen };
        (a & !net.mask() == 0).then_some(net)
    }

    pub fn mask(&self) -> u32 {
        if self.plen == 0 {
            0
        } else {
            u32::MAX << (32 - self.plen)
        }
    }

    pub fn broadcast(&self) -> u32 {
        self.addr | !self.mask()
    }

    pub fn netmask(&self) -> Ipv4Addr {
        Ipv4Addr::from(self.mask())
    }

    pub fn contains(&self, ip: u32) -> bool {
        ip & self.mask() == self.addr
    }

    pub fn subnet_of(&self, o: &Net) -> bool {
        self.plen >= o.plen && o.contains(self.addr)
    }

    pub fn overlaps(&self, o: &Net) -> bool {
        self.contains(o.addr) || o.contains(self.addr)
    }
}

impl fmt::Display for Net {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}/{}", Ipv4Addr::from(self.addr), self.plen)
    }
}

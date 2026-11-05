//! Đổi qua lại giữa "10.20.0.1/24" và cặp (địa chỉ, mặt nạ 255.255.255.0) – thay cho ipaddress.ip_interface.

use std::net::Ipv4Addr;

/// Mặt nạ chấm thập phân (255.255.255.0) hoặc số (24) -> độ dài tiền tố; None nếu không hợp lệ
/// (các bit 1 của mặt nạ phải liên tục từ bên trái).
pub fn mask_bits(mask: &str) -> Option<u32> {
    if let Ok(n) = mask.parse::<u32>() {
        return (n <= 32).then_some(n);
    }
    let v = u32::from(mask.parse::<Ipv4Addr>().ok()?);
    let n = v.leading_ones();
    (v.checked_shl(n).unwrap_or(0) == 0).then_some(n)
}

/// Độ dài tiền tố -> mặt nạ chấm thập phân.
pub fn bits_to_mask(n: u32) -> Ipv4Addr {
    Ipv4Addr::from(if n == 0 { 0 } else { u32::MAX << (32 - n) })
}

/// ("10.20.0.1", "255.255.255.0") -> "10.20.0.1/24".
pub fn to_cidr(ip: &str, mask: &str) -> Option<String> {
    let a: Ipv4Addr = ip.parse().ok()?;
    Some(format!("{a}/{}", mask_bits(mask)?))
}

/// "10.20.0.1/24" -> (10.20.0.1, 255.255.255.0).
pub fn from_cidr(cidr: &str) -> Option<(Ipv4Addr, Ipv4Addr)> {
    let (ip, plen) = cidr.split_once('/')?;
    Some((ip.parse().ok()?, bits_to_mask(mask_bits(plen)?)))
}

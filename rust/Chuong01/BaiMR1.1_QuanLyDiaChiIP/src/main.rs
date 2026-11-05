//! Bài mở rộng MR1.1 – Quản lý địa chỉ IP (IPAM) bằng std::net::Ipv4Addr.
//!
//! Bài mở rộng, không có trong giáo trình; nối tiếp Mục 1.2.3 (địa chỉ IP, tiền tố mạng)
//! và Bài 1.1. Chương trình đọc một kế hoạch địa chỉ (tệp JSON) và:
//!   1. Chia khối địa chỉ thành các mạng con có kích thước khác nhau (VLSM):
//!      yêu cầu lớn được cấp trước để các khối luôn thẳng hàng (aligned).
//!   2. Liệt kê phần địa chỉ còn trống dưới dạng các khối CIDR.
//!   3. Kiểm tra các tiền tố đề xuất: sai cú pháp, có bit máy, nằm ngoài khối, chồng lấn.
//!   4. Cấp phát địa chỉ cho thiết bị trong một mạng con (bỏ qua địa chỉ cổng mặc định).
//! Thư viện chuẩn Rust không có kiểu "mạng" như ipaddress (Python) hay netip.Prefix (Go),
//! nên bài tự viết kiểu Net gọn: địa chỉ mạng (u32) + độ dài tiền tố. Chỉ xử lý IPv4.
//!
//! Chạy:  cargo run -q --release -- [tệp_kế_hoạch.json ...]      (mặc định ke_hoach.json)

use serde_json::Value;
use std::fmt;
use std::fs;
use std::net::Ipv4Addr;
use std::process;

/// Một mạng IPv4: địa chỉ mạng (đã xóa bit máy) và độ dài tiền tố.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Net {
    addr: u32,
    plen: u32,
}

impl Net {
    fn num_addresses(&self) -> u64 {
        1u64 << (32 - self.plen)
    }
    fn first(&self) -> u32 {
        self.addr
    }
    fn last(&self) -> u32 {
        (self.addr as u64 + self.num_addresses() - 1) as u32 // địa chỉ quảng bá
    }
    fn netmask(&self) -> Ipv4Addr {
        let m = if self.plen == 0 { 0 } else { u32::MAX << (32 - self.plen) };
        Ipv4Addr::from(m)
    }
    /// Hai mạng chồng lấn khi khoảng [first, last] của chúng giao nhau.
    fn overlaps(&self, o: &Net) -> bool {
        self.first() <= o.last() && o.first() <= self.last()
    }
    /// self nằm trọn trong o.
    fn subnet_of(&self, o: &Net) -> bool {
        o.first() <= self.first() && self.last() <= o.last()
    }
}

impl fmt::Display for Net {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // dùng pad để căn cột {:<16} hoạt động như với chuỗi
        f.pad(&format!("{}/{}", Ipv4Addr::from(self.addr), self.plen))
    }
}

/// Phân tích "a.b.c.d/n". strict = true: từ chối khi còn bit máy (như ip_network(strict=True)).
fn parse_net(text: &str, strict: bool) -> Result<Net, String> {
    let (ip, plen) = match text.split_once('/') {
        Some((a, b)) => (a, b),
        None => (text, "32"),
    };
    let ip: Ipv4Addr = ip.parse().map_err(|_| format!("địa chỉ sai: {ip}"))?;
    let plen: u32 = plen.parse().map_err(|_| format!("tiền tố sai: {plen}"))?;
    if plen > 32 {
        return Err(format!("tiền tố sai: {plen}"));
    }
    let mask = if plen == 0 { 0 } else { u32::MAX << (32 - plen) };
    let a = u32::from(ip);
    if strict && a & !mask != 0 {
        return Err(format!("{text} có bit máy"));
    }
    Ok(Net { addr: a & mask, plen })
}

/// Độ dài tiền tố nhỏ nhất chứa được n_hosts máy (+2 địa chỉ mạng và quảng bá).
fn prefix_for_hosts(n_hosts: u64) -> u32 {
    let mut size: u64 = 4; // khối nhỏ nhất dùng ở đây là /30
    while size - 2 < n_hosts {
        size *= 2;
    }
    32 - size.trailing_zeros()
}

/// Một mạng con đã cấp: tên, số máy cần, mạng.
struct Alloc {
    name: String,
    need: u64,
    net: Net,
}

/// Cấp mạng con cho từng yêu cầu (lớn trước nhỏ sau).
fn plan_vlsm(block: &Net, requests: &[(String, u64)]) -> Result<Vec<Alloc>, String> {
    let mut order: Vec<&(String, u64)> = requests.iter().collect();
    order.sort_by(|a, b| b.1.cmp(&a.1)); // sắp xếp ổn định: cùng số máy giữ thứ tự tệp
    let mut cursor = block.first() as u64;
    let end = block.last() as u64;
    let mut result = Vec::new();
    for (name, need) in order {
        let plen = prefix_for_hosts(*need);
        let size = 1u64 << (32 - plen);
        cursor = (cursor + size - 1) / size * size; // làm tròn lên bội của kích thước khối
        if cursor + size - 1 > end {
            return Err(format!("Không đủ chỗ cho {name} ({need} máy cần /{plen}) trong {block}"));
        }
        result.push(Alloc { name: name.clone(), need: *need, net: Net { addr: cursor as u32, plen } });
        cursor += size;
    }
    Ok(result)
}

/// Chia khoảng [lo, hi] thành ít khối CIDR thẳng hàng nhất.
fn range_to_cidr(mut lo: u64, hi: u64) -> Vec<Net> {
    let mut out = Vec::new();
    while lo <= hi {
        // khối lớn nhất bắt đầu tại lo (thẳng hàng) và không vượt quá hi
        let mut size = if lo == 0 { 1u64 << 32 } else { 1u64 << lo.trailing_zeros().min(32) };
        while lo + size - 1 > hi {
            size /= 2;
        }
        out.push(Net { addr: lo as u32, plen: 32 - size.trailing_zeros() });
        lo += size;
    }
    out
}

/// Các khối CIDR còn trống trong block sau khi trừ đi các mạng đã dùng.
fn free_blocks(block: &Net, used: &[Net]) -> Vec<Net> {
    let mut used: Vec<Net> = used.to_vec();
    used.sort();
    let mut free = Vec::new();
    let mut cursor = block.first() as u64;
    for u in used {
        if (u.first() as u64) > cursor {
            free.extend(range_to_cidr(cursor, u.first() as u64 - 1));
        }
        cursor = cursor.max(u.last() as u64 + 1);
    }
    if cursor <= block.last() as u64 {
        free.extend(range_to_cidr(cursor, block.last() as u64));
    }
    free
}

/// Kiểm tra một tiền tố đề xuất, trả về chuỗi kết luận.
fn check_prefix(text: &str, block: &Net, allocated: &[Alloc]) -> String {
    let (net, note) = match parse_net(text, true) {
        Ok(n) => (n, String::new()),
        Err(_) => match parse_net(text, false) {
            // thử lại, chấp nhận bit máy
            Ok(n) => (n, format!("có bit máy, chuẩn hóa {n}; ")),
            Err(_) => return "KHÔNG HỢP LỆ (sai cú pháp địa chỉ hoặc tiền tố)".to_string(),
        },
    };
    if !net.subnet_of(block) {
        return format!("{note}NGOÀI KHỐI {block}");
    }
    for a in allocated {
        if net.overlaps(&a.net) {
            return format!("{note}CHỒNG LẤN với {} ({})", a.name, a.net);
        }
    }
    format!("{note}HỢP LỆ, còn trống")
}

/// Địa chỉ khả dụng đầu tiên làm cổng mặc định; các thiết bị nhận các địa chỉ tiếp theo.
fn assign_hosts(net: &Net, devices: &[String]) -> Result<Vec<(String, Ipv4Addr)>, String> {
    let mut next = net.first() + 1; // bỏ địa chỉ mạng
    let last_host = net.last() - 1; // bỏ địa chỉ quảng bá
    let mut result = vec![("gateway".to_string(), Ipv4Addr::from(next))];
    for d in devices {
        next += 1;
        if next > last_host {
            return Err(format!("Mạng {net} đã hết địa chỉ khi cấp cho {d}"));
        }
        result.push((d.clone(), Ipv4Addr::from(next)));
    }
    Ok(result)
}

fn is_private(n: &Net) -> bool {
    let p = |t: &str| parse_net(t, true).unwrap();
    [p("10.0.0.0/8"), p("172.16.0.0/12"), p("192.168.0.0/16")].iter().any(|r| n.subnet_of(r))
}

/// Xử lý một tệp kế hoạch; trả về false nếu kế hoạch không thực hiện được.
fn run_plan(path: &str) -> Result<bool, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("Không đọc được {path}: {e}"))?;
    let plan: Value = serde_json::from_str(&text).map_err(|e| format!("JSON sai trong {path}: {e}"))?;
    println!("===== Kế hoạch: {path} =====");
    let block = parse_net(plan["khoi_dia_chi"].as_str().unwrap_or(""), true)?;
    let kind = if is_private(&block) { "riêng (RFC 1918)" } else { "công cộng" };
    println!("[ipam] Khối địa chỉ {block}: {} địa chỉ, {kind}", block.num_addresses());
    let requests: Vec<(String, u64)> = plan["yeu_cau"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|r| (r["ten"].as_str().unwrap_or("").to_string(), r["so_may"].as_u64().unwrap_or(0)))
                .collect()
        })
        .unwrap_or_default();
    let allocated = match plan_vlsm(&block, &requests) {
        Ok(a) => a,
        Err(e) => {
            println!("[ipam] LỖI: {e}");
            return Ok(false);
        }
    };

    println!("\n[ipam] 1) Chia mạng con (VLSM, yêu cầu lớn cấp trước)");
    println!("  {:<14} {:>4} {:>8} {:<16} {:<15} Dải khả dụng", "Tên", "Cần", "Khả dụng", "Mạng con", "Mặt nạ");
    for a in &allocated {
        let (first, last) = (Ipv4Addr::from(a.net.first() + 1), Ipv4Addr::from(a.net.last() - 1));
        println!(
            "  {:<14} {:>4} {:>8} {:<16} {:<15} {first} - {last}",
            a.name,
            a.need,
            a.net.num_addresses() - 2,
            a.net,
            a.net.netmask().to_string()
        );
    }
    let used: u64 = allocated.iter().map(|a| a.net.num_addresses()).sum();
    let total = block.num_addresses();
    println!("  Đã dùng {used}/{total} địa chỉ ({:.1}%)", used as f64 * 100.0 / total as f64);

    println!("\n[ipam] 2) Khối còn trống");
    let nets: Vec<Net> = allocated.iter().map(|a| a.net).collect();
    for f in free_blocks(&block, &nets) {
        println!("  {:<17} ({} địa chỉ)", f, f.num_addresses());
    }

    if let Some(list) = plan["kiem_tra"].as_array().filter(|l| !l.is_empty()) {
        println!("\n[ipam] 3) Kiểm tra tiền tố đề xuất");
        for t in list {
            let t = t.as_str().unwrap_or("");
            println!("  {:<17} -> {}", t, check_prefix(t, &block, &allocated));
        }
    }

    if let Some(req) = plan["cap_phat"].as_object() {
        let want = req["mang"].as_str().unwrap_or("");
        let net = allocated.iter().find(|a| a.name == want).ok_or(format!("Không có mạng {want}"))?.net;
        println!("\n[ipam] 4) Cấp phát địa chỉ trong {want} ({net})");
        let devices: Vec<String> = req["thiet_bi"]
            .as_array()
            .map(|a| a.iter().filter_map(|d| d.as_str().map(String::from)).collect())
            .unwrap_or_default();
        for (dev, ip) in assign_hosts(&net, &devices)? {
            println!("  {:<8} {ip}/{}", dev, net.plen);
        }
    }
    Ok(true)
}

fn main() {
    let mut paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        paths.push("ke_hoach.json".to_string());
    }
    let mut all_ok = true;
    for p in &paths {
        match run_plan(p) {
            Ok(ok) => all_ok &= ok,
            Err(e) => {
                println!("[ipam] LỖI: {e}");
                all_ok = false;
            }
        }
    }
    process::exit(if all_ok { 0 } else { 1 });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cidr_cua_khoang_trong() {
        let v: Vec<String> = range_to_cidr(0x0A1401E4, 0x0A1403FF).iter().map(|n| n.to_string()).collect();
        assert_eq!(v, ["10.20.1.228/30", "10.20.1.232/29", "10.20.1.240/28", "10.20.2.0/23"]);
    }

    #[test]
    fn tien_to_theo_so_may() {
        assert_eq!(prefix_for_hosts(2), 30);
        assert_eq!(prefix_for_hosts(200), 24);
        assert_eq!(prefix_for_hosts(254), 24);
        assert_eq!(prefix_for_hosts(255), 23);
    }
}

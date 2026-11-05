//! Định dạng NetFlow phiên bản 5 – dùng chung cho máy xuất và bộ thu thập.
//!
//! Một gói xuất (export packet) = tiêu đề 24 byte + count bản ghi, mỗi bản ghi 48 byte,
//! mọi trường theo thứ tự byte mạng (big-endian: to_be_bytes / from_be_bytes).
//! Tiêu đề: version, count, sys_uptime, unix_secs, unix_nsecs, flow_sequence,
//!          engine_type, engine_id, sampling_interval.
//! Bản ghi: srcaddr, dstaddr, nexthop, input, output, dPkts, dOctets, first, last,
//!          srcport, dstport, pad1, tcp_flags, prot, tos, src_as, dst_as, src_mask, dst_mask, pad2.
//! Python dùng struct.Struct("!HHIIIIBBH"); Rust không có mô-đun tương đương trong thư viện chuẩn
//! nên tự ghi/đọc từng trường theo đúng vị trí byte (offset).

use std::net::Ipv4Addr;

pub const HEADER_SIZE: usize = 24;
pub const RECORD_SIZE: usize = 48;
pub const MAX_RECORDS: usize = 30; // giới hạn của phiên bản 5

pub fn proto_name(p: u8) -> String {
    match p {
        6 => "TCP".into(),
        17 => "UDP".into(),
        1 => "ICMP".into(),
        other => other.to_string(),
    }
}

pub struct Header {
    pub version: u16,
    pub count: u16,
    pub sys_uptime: u32,
    pub unix_secs: u32,
    pub unix_nsecs: u32,
    pub flow_sequence: u32,
    pub engine_type: u8,
    pub engine_id: u8,
    pub sampling: u16,
}

pub struct Record {
    pub src: Ipv4Addr,
    pub dst: Ipv4Addr,
    pub nexthop: Ipv4Addr,
    pub input: u16,
    pub output: u16,
    pub pkts: u32,
    pub octets: u32,
    pub first: u32,
    pub last: u32,
    pub sport: u16,
    pub dport: u16,
    pub tcp_flags: u8,
    pub proto: u8,
    pub tos: u8,
    pub src_as: u16,
    pub dst_as: u16,
    pub src_mask: u8,
    pub dst_mask: u8,
}

impl Header {
    /// "!HHIIIIBBH" – 24 byte.
    pub fn pack(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.version.to_be_bytes());
        out.extend_from_slice(&self.count.to_be_bytes());
        out.extend_from_slice(&self.sys_uptime.to_be_bytes());
        out.extend_from_slice(&self.unix_secs.to_be_bytes());
        out.extend_from_slice(&self.unix_nsecs.to_be_bytes());
        out.extend_from_slice(&self.flow_sequence.to_be_bytes());
        out.push(self.engine_type);
        out.push(self.engine_id);
        out.extend_from_slice(&self.sampling.to_be_bytes());
    }

    /// Đọc tiêu đề từ đầu datagram (đã kiểm tra đủ 24 byte).
    pub fn unpack(d: &[u8]) -> Header {
        Header {
            version: be16(d, 0),
            count: be16(d, 2),
            sys_uptime: be32(d, 4),
            unix_secs: be32(d, 8),
            unix_nsecs: be32(d, 12),
            flow_sequence: be32(d, 16),
            engine_type: d[20],
            engine_id: d[21],
            sampling: be16(d, 22),
        }
    }
}

impl Record {
    /// "!4s4s4sHHIIIIHHBBBBHHBBH" – 48 byte.
    pub fn pack(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.src.octets());
        out.extend_from_slice(&self.dst.octets());
        out.extend_from_slice(&self.nexthop.octets());
        out.extend_from_slice(&self.input.to_be_bytes());
        out.extend_from_slice(&self.output.to_be_bytes());
        out.extend_from_slice(&self.pkts.to_be_bytes());
        out.extend_from_slice(&self.octets.to_be_bytes());
        out.extend_from_slice(&self.first.to_be_bytes());
        out.extend_from_slice(&self.last.to_be_bytes());
        out.extend_from_slice(&self.sport.to_be_bytes());
        out.extend_from_slice(&self.dport.to_be_bytes());
        out.push(0); // pad1
        out.push(self.tcp_flags);
        out.push(self.proto);
        out.push(self.tos);
        out.extend_from_slice(&self.src_as.to_be_bytes());
        out.extend_from_slice(&self.dst_as.to_be_bytes());
        out.push(self.src_mask);
        out.push(self.dst_mask);
        out.extend_from_slice(&[0, 0]); // pad2
    }

    /// Đọc một bản ghi bắt đầu tại vị trí `o`.
    pub fn unpack(d: &[u8], o: usize) -> Record {
        let ip = |k: usize| Ipv4Addr::new(d[o + k], d[o + k + 1], d[o + k + 2], d[o + k + 3]);
        Record {
            src: ip(0),
            dst: ip(4),
            nexthop: ip(8),
            input: be16(d, o + 12),
            output: be16(d, o + 14),
            pkts: be32(d, o + 16),
            octets: be32(d, o + 20),
            first: be32(d, o + 24),
            last: be32(d, o + 28),
            sport: be16(d, o + 32),
            dport: be16(d, o + 34),
            tcp_flags: d[o + 37],
            proto: d[o + 38],
            tos: d[o + 39],
            src_as: be16(d, o + 40),
            dst_as: be16(d, o + 42),
            src_mask: d[o + 44],
            dst_mask: d[o + 45],
        }
    }
}

fn be16(d: &[u8], o: usize) -> u16 {
    u16::from_be_bytes([d[o], d[o + 1]])
}

fn be32(d: &[u8], o: usize) -> u32 {
    u32::from_be_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kich_thuoc_tieu_de_va_ban_ghi() {
        let mut h = Vec::new();
        Header { version: 5, count: 1, sys_uptime: 0, unix_secs: 0, unix_nsecs: 0, flow_sequence: 7, engine_type: 0, engine_id: 1, sampling: 0 }.pack(&mut h);
        assert_eq!(h.len(), HEADER_SIZE);
        let z = Ipv4Addr::UNSPECIFIED;
        let r = Record { src: z, dst: z, nexthop: z, input: 1, output: 2, pkts: 3, octets: 4, first: 5, last: 6, sport: 7, dport: 8, tcp_flags: 9, proto: 6, tos: 0, src_as: 0, dst_as: 0, src_mask: 24, dst_mask: 27 };
        let mut b = Vec::new();
        r.pack(&mut b);
        assert_eq!(b.len(), RECORD_SIZE);
        let back = Record::unpack(&b, 0);
        assert_eq!((back.sport, back.dport, back.proto, back.dst_mask), (7, 8, 6, 27));
        assert_eq!(Header::unpack(&h).flow_sequence, 7);
    }
}

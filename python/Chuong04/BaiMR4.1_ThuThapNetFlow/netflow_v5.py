"""Định dạng NetFlow phiên bản 5 – dùng chung cho may_xuat.py và bo_thu_thap.py.

Một gói xuất (export packet) = tiêu đề 24 byte + count bản ghi, mỗi bản ghi 48 byte,
mọi trường theo thứ tự byte mạng (big-endian, ký hiệu "!" của struct).
Tiêu đề: version, count, sys_uptime, unix_secs, unix_nsecs, flow_sequence,
         engine_type, engine_id, sampling_interval.
Bản ghi: srcaddr, dstaddr, nexthop, input, output, dPkts, dOctets, first, last,
         srcport, dstport, pad1, tcp_flags, prot, tos, src_as, dst_as, src_mask, dst_mask, pad2.
"""
import struct

HEADER = struct.Struct("!HHIIIIBBH")                    # 24 byte
RECORD = struct.Struct("!4s4s4sHHIIIIHHBBBBHHBBH")      # 48 byte
MAX_RECORDS = 30                                        # giới hạn của phiên bản 5
PROTO_NAME = {6: "TCP", 17: "UDP", 1: "ICMP"}

assert HEADER.size == 24 and RECORD.size == 48

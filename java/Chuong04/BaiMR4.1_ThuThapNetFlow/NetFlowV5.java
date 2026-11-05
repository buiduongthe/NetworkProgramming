import java.util.Map;

/**
 * Định dạng NetFlow phiên bản 5 – dùng chung cho MayXuat và BoThuThap.
 *
 * Một gói xuất (export packet) = tiêu đề 24 byte + count bản ghi, mỗi bản ghi 48 byte,
 * mọi trường theo thứ tự byte mạng (big-endian – thứ tự mặc định của java.nio.ByteBuffer).
 * Tiêu đề: version(2), count(2), sys_uptime(4), unix_secs(4), unix_nsecs(4), flow_sequence(4),
 *          engine_type(1), engine_id(1), sampling_interval(2).
 * Bản ghi: srcaddr(4), dstaddr(4), nexthop(4), input(2), output(2), dPkts(4), dOctets(4), first(4), last(4),
 *          srcport(2), dstport(2), pad1(1), tcp_flags(1), prot(1), tos(1), src_as(2), dst_as(2),
 *          src_mask(1), dst_mask(1), pad2(2).
 */
final class NetFlowV5 {
    private NetFlowV5() {}

    static final int HEADER_SIZE = 24;
    static final int RECORD_SIZE = 48;
    static final int MAX_RECORDS = 30;                 // giới hạn của phiên bản 5
    static final Map<Integer, String> PROTO_NAME = Map.of(6, "TCP", 17, "UDP", 1, "ICMP");

    /** Vị trí (offset) các trường cần dùng trong bản ghi. */
    static final int R_SRC = 0, R_DST = 4, R_NEXTHOP = 8, R_INPUT = 12, R_OUTPUT = 14, R_PKTS = 16, R_OCTETS = 20,
            R_FIRST = 24, R_LAST = 28, R_SPORT = 32, R_DPORT = 34, R_FLAGS = 37, R_PROTO = 38,
            R_SRC_MASK = 44, R_DST_MASK = 45;
}

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.IDN;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.SocketTimeoutException;
import java.nio.charset.StandardCharsets;
import java.security.SecureRandom;
import java.util.ArrayList;
import java.util.List;

/**
 * Bài 4.3 – Tự tạo và phân tích truy vấn DNS qua UDP (Mã giả 4.5, Mục 4.3.2 Giáo trình Lập trình mạng).
 * Bổ sung so với mã giả (theo đoạn văn sau Mã giả 4.5): kiểm tra chỉ số pos và cờ TC.
 * Biên dịch: javac -encoding UTF-8 -d out DnsQuery.java
 * Chạy:      java -cp out DnsQuery <ip_dns_server> <ten_mien> [A|AAAA] [cong]   (mặc định A, cổng 53)
 */
public class DnsQuery {
    static final String[] RCODES = {"NOERROR", "FORMERR", "SERVFAIL", "NXDOMAIN", "NOTIMP", "REFUSED"};

    static class ProtocolError extends Exception {
        ProtocolError(String msg) { super(msg); }
    }

    static String hex(byte[] b, int n) {
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < n; i++) sb.append(i > 0 ? " " : "").append(String.format("%02X", b[i] & 0xFF));
        return sb.toString();
    }

    static int u16(byte[] m, int pos) { return ((m[pos] & 0xFF) << 8) | (m[pos + 1] & 0xFF); } // FROM_UINT16_BE

    static void u16be(ByteArrayOutputStream q, int v) { q.write(v >> 8); q.write(v); }   // UINT16_BE

    static byte[] buildQuery(int id, String name, int qtype) {                           // dòng 1–9
        ByteArrayOutputStream q = new ByteArrayOutputStream();
        u16be(q, id); u16be(q, 0x0100);                                                  // dòng 2: RD = 1
        u16be(q, 1); q.writeBytes(new byte[6]);                                          // dòng 3: QD=1, AN=NS=AR=0
        for (String label : name.replaceAll("\\.$", "").split("\\.")) {                  // dòng 4
            byte[] b = IDN.toASCII(label).getBytes(StandardCharsets.US_ASCII);           // dòng 5: Punycode
            if (b.length < 1 || b.length > 63) throw new IllegalArgumentException("Nhãn không hợp lệ: '" + label + "'");
            q.write(b.length); q.writeBytes(b);                                          // dòng 6
        }
        q.write(0); u16be(q, qtype); u16be(q, 1);                                        // dòng 8: QCLASS = IN
        return q.toByteArray();
    }

    /** Kiểm tra chỉ số (bổ sung): phản hồi sai định dạng không được làm đọc ngoài thông điệp. */
    static void need(int len, int end) throws ProtocolError {
        if (end > len) throw new ProtocolError("Phản hồi sai định dạng: vượt quá độ dài thông điệp");
    }

    static int skipName(byte[] m, int len, int pos) throws ProtocolError {               // dòng 10–17
        while (true) {
            need(len, pos + 1);
            int n = m[pos] & 0xFF;
            if (n == 0) return pos + 1;                                                  // hết tên
            if ((n & 0xC0) == 0xC0) { need(len, pos + 2); return pos + 2; }              // con trỏ nén
            if ((n & 0xC0) != 0) throw new ProtocolError("Phản hồi sai định dạng: nhãn kiểu lạ");
            pos += 1 + n;
        }
    }

    /** IPV6_TEXT dạng rút gọn RFC 5952 (getHostAddress() của Java không rút gọn chuỗi số 0). */
    static String ipv6Text(byte[] a) {
        int[] g = new int[8];
        for (int i = 0; i < 8; i++) g[i] = u16(a, 2 * i);
        int best = -1, bestLen = 1;
        for (int i = 0; i < 8; i++) {
            int j = i;
            while (j < 8 && g[j] == 0) j++;
            if (j - i > bestLen) { best = i; bestLen = j - i; }
        }
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < 8; i++) {
            if (i == best) { sb.append("::"); i += bestLen - 1; continue; }
            if (sb.length() > 0 && sb.charAt(sb.length() - 1) != ':') sb.append(':');
            sb.append(Integer.toHexString(g[i]));
        }
        return sb.toString();
    }

    /** server = (ip, cổng). DatagramSocket của Java là socket hai ngăn xếp (dual-stack): cùng một socket
     *  gửi được tới địa chỉ IPv4 lẫn IPv6, tương ứng SOCKET(FAMILY(server.ip), DGRAM) ở dòng 19. */
    static List<String> query(InetSocketAddress server, String name, int qtype, String tname)
            throws IOException, ProtocolError {
        byte[] m = new byte[4096];
        DatagramPacket p = new DatagramPacket(m, m.length);
        int id = new SecureRandom().nextInt(65536);                                      // dòng 20: RANDOM_UINT16()
        byte[] q = buildQuery(id, name, qtype);
        System.out.println("[client] Truy vấn " + tname + " cho " + name + " tới " + server.getAddress().getHostAddress() + ":" + server.getPort()
                + " (id=0x" + String.format("%04X", id) + ", " + q.length + " byte)");
        System.out.println("[client] Truy vấn (hex): " + hex(q, q.length));
        try (DatagramSocket s = new DatagramSocket()) {                                  // dòng 19; CLOSE khi rời try
            s.setSoTimeout(2000);                                                        // dòng 20: RECV_TIMEOUT
            s.send(new DatagramPacket(q, q.length, server));                             // dòng 21
            s.receive(p);                                                                // dòng 22
        }
        int len = p.getLength();
        String from = p.getAddress().getHostAddress() + ":" + p.getPort();
        if (!p.getSocketAddress().equals(server)) throw new ProtocolError("Phản hồi từ địa chỉ lạ " + from); // dòng 23
        need(len, 12);
        if (u16(m, 0) != id) throw new ProtocolError("ID của phản hồi không khớp truy vấn");  // dòng 24
        int rcode = m[3] & 0x0F;                                                         // dòng 25: 3 = NXDOMAIN
        int ancount = u16(m, 6);                                                         // dòng 26
        System.out.println("[client] Phản hồi " + len + " byte từ " + from + ", RCODE=" + rcode + " ("
                + (rcode < RCODES.length ? RCODES[rcode] : "?") + "), ANCOUNT=" + ancount);
        System.out.println("[client] Phản hồi (hex): " + hex(m, len));
        if ((m[2] & 0x02) != 0)                                                          // bổ sung: cờ TC
            throw new ProtocolError("Phản hồi bị cắt (TC = 1): cần hỏi lại qua TCP cổng 53");
        int pos = skipName(m, len, 12) + 4;                                              // dòng 27
        List<String> answers = new ArrayList<>();
        for (int i = 0; i < ancount; i++) {                                              // dòng 29
            pos = skipName(m, len, pos);                                                 // dòng 30
            need(len, pos + 10);
            int type = u16(m, pos);                                                      // dòng 31
            int rdlen = u16(m, pos + 8);                                                 // dòng 32
            need(len, pos + 10 + rdlen);
            byte[] rdata = java.util.Arrays.copyOfRange(m, pos + 10, pos + 10 + rdlen);  // dòng 33
            if (type == 1 && rdlen == 4) answers.add(InetAddress.getByAddress(rdata).getHostAddress()); // dòng 34
            if (type == 28 && rdlen == 16) answers.add(ipv6Text(rdata));                 // dòng 35
            pos += 10 + rdlen;                                                           // dòng 36
        }
        return answers;
    }

    public static void main(String[] args) throws IOException {
        if (args.length < 2) {
            System.out.println("Cách dùng: java -cp out DnsQuery <ip_dns_server> <ten_mien> [A|AAAA] [cong]");
            System.exit(1);
        }
        String tname = args.length > 2 ? args[2].toUpperCase() : "A";
        int port = args.length > 3 ? Integer.parseInt(args[3]) : 53;
        int qtype = tname.equals("A") ? 1 : tname.equals("AAAA") ? 28 : 0;
        if (qtype == 0) { System.out.println("Kiểu bản ghi chỉ hỗ trợ A hoặc AAAA"); System.exit(1); }
        try {
            List<String> answers = query(new InetSocketAddress(InetAddress.getByName(args[0]), port), args[1], qtype, tname);
            for (String a : answers) System.out.println("[client] Địa chỉ: " + a);
            if (answers.isEmpty()) System.out.println("[client] Không có bản ghi " + tname);
        } catch (SocketTimeoutException e) {
            System.out.println("[client] Lỗi: hết thời gian chờ phản hồi (2000 ms)");
            System.exit(1);
        } catch (ProtocolError e) {
            System.out.println("[client] Lỗi giao thức: " + e.getMessage());
            System.exit(1);
        }
    }
}

import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;

/**
 * Bài mở rộng MR4.1 – Máy xuất NetFlow v5 giả lập (exporter) gửi qua UDP.
 *
 * Sinh các bản ghi luồng (flow) giả lập theo một bộ sinh số giả ngẫu nhiên đơn giản (LCG)
 * có hạt giống cố định, nên mọi bản ngôn ngữ sinh ra đúng cùng một dữ liệu.
 * Mỗi gói UDP chứa tối đa 30 bản ghi. Tùy chọn --bo-goi K cố ý KHÔNG gửi gói thứ K
 * (mô phỏng mất gói trên đường truyền); cuối cùng gửi thêm một datagram rác để thử bộ thu.
 *
 * Chạy: java -cp out MayXuat <host> <port> [--so-luong N] [--bo-goi K]
 */
public class MayXuat {
    static final String[] HOSTS = {"10.20.0.11", "10.20.0.12", "10.20.0.13", "10.20.0.14", "10.20.1.5", "10.20.1.6"};
    record Service(String dst, int port, int proto) {}
    static final Service[] SERVICES = {new Service("10.20.1.194", 443, 6), new Service("10.20.1.194", 80, 6),
            new Service("10.20.1.195", 5432, 6), new Service("10.20.1.196", 53, 17), new Service("203.0.113.10", 443, 6)};

    /**
     * x = (x * 1103515245 + 12345) mod 2^31, lấy 15 bit cao (các bit thấp của LCG kém ngẫu nhiên).
     * Đủ để sinh dữ liệu thử, KHÔNG dùng cho bảo mật.
     */
    static final class Lcg {
        long x;
        Lcg(long seed) { x = seed; }
        int next() {
            x = (x * 1103515245L + 12345) & 0x7FFFFFFFL;
            return (int) (x >> 16);
        }
    }

    record Flow(String src, String dst, int sport, int dport, int proto, long pkts, long octets, long first, long last) {}

    static List<Flow> makeFlows(int n, long seed) {
        Lcg rng = new Lcg(seed);
        List<Flow> flows = new ArrayList<>();
        for (int i = 0; i < n; i++) {
            String src = HOSTS[rng.next() % HOSTS.length];
            Service s = SERVICES[rng.next() % SERVICES.length];
            long pkts = 1 + rng.next() % 200;
            long octets = pkts * (60 + rng.next() % 1400);
            int sport = 32768 + rng.next() % 28000;
            long first = rng.next() % 60000;
            flows.add(new Flow(src, s.dst(), sport, s.port(), s.proto(), pkts, octets, first, first + rng.next() % 5000));
        }
        return flows;
    }

    static byte[] ip4(String s) throws Exception { return InetAddress.getByName(s).getAddress(); }   // chuỗi số: không tra DNS

    static byte[] packPacket(List<Flow> flows, long seq, long uptimeMs) throws Exception {
        ByteBuffer b = ByteBuffer.allocate(NetFlowV5.HEADER_SIZE + flows.size() * NetFlowV5.RECORD_SIZE);  // big-endian
        long nowMs = System.currentTimeMillis();
        b.putShort((short) 5).putShort((short) flows.size()).putInt((int) uptimeMs)
         .putInt((int) (nowMs / 1000)).putInt((int) (nowMs % 1000 * 1_000_000)).putInt((int) seq)
         .put((byte) 0).put((byte) 1).putShort((short) 0);                 // engine_type, engine_id, sampling
        for (Flow f : flows) {
            b.put(ip4(f.src())).put(ip4(f.dst())).put(ip4("0.0.0.0"))
             .putShort((short) 1).putShort((short) 2)                       // input, output
             .putInt((int) f.pkts()).putInt((int) f.octets()).putInt((int) f.first()).putInt((int) f.last())
             .putShort((short) f.sport()).putShort((short) f.dport())
             .put((byte) 0).put((byte) (f.proto() == 6 ? 0x1B : 0)).put((byte) f.proto()).put((byte) 0)  // pad1, flags, prot, tos
             .putShort((short) 0).putShort((short) 0)                       // src_as, dst_as
             .put((byte) 24).put((byte) 27).putShort((short) 0);            // src_mask, dst_mask, pad2
        }
        return b.array();
    }

    public static void main(String[] args) throws Exception {
        String host = args[0];
        int port = Integer.parseInt(args[1]);
        int soLuong = 100, boGoi = 0;
        for (int i = 2; i + 1 < args.length; i += 2) {
            if (args[i].equals("--so-luong")) soLuong = Integer.parseInt(args[i + 1]);
            else if (args[i].equals("--bo-goi")) boGoi = Integer.parseInt(args[i + 1]);
        }
        List<Flow> flows = makeFlows(soLuong, 2026);
        InetSocketAddress dest = new InetSocketAddress(host, port);
        try (DatagramSocket sock = new DatagramSocket()) {
            long seq = 0;
            int k = 0;
            for (int start = 0; start < flows.size(); start += NetFlowV5.MAX_RECORDS) {
                k++;
                List<Flow> chunk = flows.subList(start, Math.min(flows.size(), start + NetFlowV5.MAX_RECORDS));
                byte[] data = packPacket(chunk, seq, 120000 + k * 1000L);
                if (k == boGoi) {
                    System.out.println("[xuat] Gói #" + k + ": " + chunk.size() + " bản ghi, seq=" + seq + " -> BỎ (giả lập mất gói)");
                } else {
                    sock.send(new DatagramPacket(data, data.length, dest));   // SENDTO
                    System.out.println("[xuat] Gói #" + k + ": " + chunk.size() + " bản ghi, seq=" + seq + ", " + data.length + " byte");
                }
                seq += chunk.size();                      // flow_sequence đếm tổng số luồng đã xuất
                Thread.sleep(50);
            }
            byte[] junk = "xin chao, day khong phai NetFlow".getBytes(StandardCharsets.UTF_8);
            sock.send(new DatagramPacket(junk, junk.length, dest));
            System.out.println("[xuat] Gửi thêm 1 datagram rác để thử bộ thu");
        }
    }
}

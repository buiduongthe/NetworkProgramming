import java.io.IOException;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.PortUnreachableException;
import java.net.SocketTimeoutException;
import java.nio.ByteBuffer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/**
 * Bài 4.4 – Bên gửi dừng và chờ (Mã giả 4.6 SAW_SEND, Mục 4.4.1) với RTO theo RFC 6298 (Mục 4.4.2).
 * Chạy: java -cp out SawSend <tep> [host] [port] [mat_goi]   (mặc định 127.0.0.1 9000, không mất gói)
 */
public class SawSend {
    static double now() { return System.nanoTime() / 1e6; }

    /** RTO() / UPDATE_RTT() / BACKOFF_RTO() theo RFC 6298. */
    static final class RttEstimator {
        static final double ALPHA = 1.0 / 8, BETA = 1.0 / 4, K = 4, G = 1.0;   // G: độ phân giải đồng hồ 1 ms
        Double srtt = null;
        double rttvar, rto = SawCommon.RTO_INIT;

        void update(double r) {
            if (srtt == null) { srtt = r; rttvar = r / 2; }                   // mẫu đầu tiên
            else {
                rttvar = (1 - BETA) * rttvar + BETA * Math.abs(srtt - r);     // tính trước, với SRTT cũ
                srtt = (1 - ALPHA) * srtt + ALPHA * r;
            }
            rto = Math.min(SawCommon.MAX_RTO, Math.max(SawCommon.RTO_MIN, srtt + Math.max(G, K * rttvar)));
        }

        void backoff() { rto = Math.min(SawCommon.MAX_RTO, 2 * rto); }       // BACKOFF_RTO(): RTO ← MIN(2 × RTO, MAX_RTO)
    }

    /** WAIT_ACK: chờ ACK đúng seq từ đúng peer trong rtoMs; bỏ ACK cũ, ACK lạ; hết giờ → SocketTimeoutException. */
    static void waitAck(DatagramSocket s, InetSocketAddress peer, int seq, double rtoMs) throws IOException {
        double deadline = now() + rtoMs;
        byte[] d = new byte[65535];
        while (true) {
            double remaining = deadline - now();
            if (remaining <= 0) throw new SocketTimeoutException();
            s.setSoTimeout((int) Math.max(1, remaining));
            DatagramPacket p = new DatagramPacket(d, d.length);
            try { s.receive(p); }
            catch (PortUnreachableException e) { throw new SocketTimeoutException(); }   // Windows: ICMP – như hết giờ
            if (p.getSocketAddress().equals(peer) && p.getLength() >= 5 && d[4] == SawCommon.ACK
                    && ByteBuffer.wrap(d, 0, 4).getInt() == seq) return;
        }
    }

    /** Mã giả 4.6 – SAW_SEND; trả về {số datagram đã gửi, số lần gửi lại}. */
    static int[] sawSend(DatagramSocket s, InetSocketAddress peer, List<byte[]> chunks, RttEstimator est,
                         SawCommon.LossSim loss) throws IOException {
        int seq = 0, sent = 0, retrans = 0;                                          // dòng 3
        List<byte[]> all = new ArrayList<>(chunks);
        all.add(null);                                                               // dòng 4: null = FIN_MARK
        for (byte[] c : all) {
            byte type = c == null ? SawCommon.FIN : SawCommon.DATA;                  // dòng 5
            byte[] body = c == null ? new byte[0] : c;
            int tries = 0;                                                           // dòng 6
            while (true) {                                                           // dòng 7: LOOP
                byte[] dgram = ByteBuffer.allocate(5 + body.length).putInt(seq).put(type).put(body).array();
                boolean dropped = loss.drop();
                if (!dropped) s.send(new DatagramPacket(dgram, dgram.length, peer)); // dòng 8
                sent++;
                System.out.println("[gửi] seq=" + seq + " " + (type == SawCommon.FIN ? "FIN" : "DATA") + " "
                        + dgram.length + " byte, RTO " + Math.round(est.rto) + " ms" + (dropped ? " – mô phỏng mất gói" : ""));
                double t0 = now();                                                   // dòng 9
                try {
                    waitAck(s, peer, seq, est.rto);                                  // dòng 11
                    if (tries == 0) est.update(now() - t0);                          // dòng 12: Karn
                    break;                                                           // dòng 13
                } catch (SocketTimeoutException e) {                                 // dòng 14
                    tries++; retrans++;
                    System.out.println("[gửi] Hết " + Math.round(est.rto) + " ms chờ ACK " + seq + ", gửi lại (lần " + tries + ")");
                    est.backoff();                                                   // dòng 16
                    if (tries > SawCommon.MAX_TRIES)                                 // dòng 17
                        throw new IOException("Không nhận được ACK " + seq + " sau " + SawCommon.MAX_TRIES + " lần gửi lại");
                }
            }
            seq++;                                                                   // dòng 20
        }
        return new int[] {sent, retrans};
    }

    public static void main(String[] args) throws IOException {
        if (args.length < 1) { System.out.println("Cách dùng: java -cp out SawSend <tep> [host] [port] [mat_goi]"); System.exit(1); }
        String path = args[0];
        Object[] ep = SawCommon.parseEndpoint(Arrays.copyOfRange(args, 1, Math.min(args.length, 3)), "127.0.0.1", 9000);
        String host = (String) ep[0];
        int port = (Integer) ep[1];
        SawCommon.LossSim loss = new SawCommon.LossSim(args.length > 3 ? args[3] : "");
        byte[] data = Files.readAllBytes(Path.of(path));
        List<byte[]> chunks = new ArrayList<>();                                     // các khối ≤ 1195 byte
        for (int i = 0; i < data.length; i += SawCommon.CHUNK)
            chunks.add(Arrays.copyOfRange(data, i, Math.min(data.length, i + SawCommon.CHUNK)));
        System.out.println("[gửi] Tệp " + Path.of(path).getFileName() + ": " + data.length + " byte, " + chunks.size() + " khối; SHA-256:");
        System.out.println("[gửi]   " + SawCommon.sha256File(path));
        System.out.println("[gửi] Gửi tới " + host + ":" + port + "; mô phỏng mất DATA/FIN: " + (loss.spec.isEmpty() ? "không" : loss.spec));
        System.out.println("[gửi] " + SawCommon.maxRtoNote());
        InetSocketAddress peer = new InetSocketAddress(InetAddress.getAllByName(host)[0], port);
        try (DatagramSocket s = new DatagramSocket()) {
            double t0 = now();
            int[] r = sawSend(s, peer, chunks, new RttEstimator(), loss);
            System.out.println("[gửi] Hoàn tất: " + r[0] + " datagram đã gửi (" + r[1] + " lần gửi lại), " + Math.round(now() - t0) + " ms");
        } catch (IOException e) {
            System.out.println("[gửi] Lỗi: " + e.getMessage());
            System.exit(1);
        }
    }
}

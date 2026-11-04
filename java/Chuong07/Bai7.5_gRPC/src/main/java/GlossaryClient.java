import glossary.v1.GlossaryGrpc;
import glossary.v1.LookupRequest;
import glossary.v1.Term;
import glossary.v1.WatchRequest;
import io.grpc.Grpc;
import io.grpc.InsecureChannelCredentials;
import io.grpc.ManagedChannel;
import io.grpc.StatusRuntimeException;
import java.util.HexFormat;
import java.util.Iterator;
import java.util.concurrent.TimeUnit;

/**
 * Bài 7.5 – Gọi dịch vụ gRPC tra cứu thuật ngữ, phía client (Mã giả 7.6, phần "Phía client").
 * Dùng stub chặn (blocking stub) do protoc-gen-grpc-java sinh: Lookup có hạn chót, Watch trả về Iterator.
 * Chạy: mvn -q compile exec:java -Dexec.mainClass=GlossaryClient -Dexec.args="[host] [port] [hạn_chót_ms]"
 *       (mặc định 127.0.0.1 5001 2000)
 */
public class GlossaryClient {
    public static void main(String[] args) throws InterruptedException {
        String host = args.length >= 1 ? args[0] : "127.0.0.1";
        int port = args.length >= 2 ? Integer.parseInt(args[1]) : 5001;
        int deadlineMs = args.length >= 3 ? Integer.parseInt(args[2]) : 2000;
        byte[] wire = LookupRequest.newBuilder().setTerm("TCP").build().toByteArray();   // Protocol Buffers (Mục 7.4.2)
        System.out.println("[client] LookupRequest{term: \"TCP\"} = "
                + HexFormat.ofDelimiter(" ").withUpperCase().formatHex(wire) + " (" + wire.length + " byte)");
        ManagedChannel ch = Grpc.newChannelBuilderForAddress(host, port, InsecureChannelCredentials.create()).build();
        try {
            GlossaryGrpc.GlossaryBlockingStub c = GlossaryGrpc.newBlockingStub(ch);    // GlossaryClient(ch)
            System.out.println("[client] Gọi Lookup với hạn chót " + deadlineMs + " ms");
            lookup(c, "TCP", deadlineMs);
            lookup(c, "XYZ", deadlineMs);
            System.out.println("[client] Gọi Watch(prefix='HTTP'), nhận luồng:");
            try {
                Iterator<Term> it = c.watch(WatchRequest.newBuilder().setPrefix("HTTP").build());
                while (it.hasNext()) {                                  // FOR EACH x IN AWAIT_STREAM(...)
                    Term x = it.next();
                    System.out.println("[client]   " + x.getTerm() + " – " + x.getText());
                }
            } catch (StatusRuntimeException e) {
                System.out.println("[client] Watch -> lỗi " + e.getStatus().getCode() + ": " + e.getStatus().getDescription());
            }
        } finally {
            ch.shutdown().awaitTermination(3, TimeUnit.SECONDS);
        }
        System.out.println("[client] Kết thúc");
    }

    static void lookup(GlossaryGrpc.GlossaryBlockingStub c, String term, int deadlineMs) {
        try {
            Term t = c.withDeadlineAfter(deadlineMs, TimeUnit.MILLISECONDS)     // deadline ← 2000
                    .lookup(LookupRequest.newBuilder().setTerm(term).build());
            System.out.println("[client] Lookup('" + term + "') -> " + t.getTerm() + ": " + t.getText());
        } catch (StatusRuntimeException e) {                           // NOT_FOUND, DEADLINE_EXCEEDED, UNAVAILABLE…
            System.out.println("[client] Lookup('" + term + "') -> lỗi " + e.getStatus().getCode() + ": "
                    + e.getStatus().getDescription());
        }
    }
}

import glossary.v1.GlossaryGrpc;
import glossary.v1.LookupRequest;
import glossary.v1.Term;
import glossary.v1.WatchRequest;
import io.grpc.Grpc;
import io.grpc.InsecureServerCredentials;
import io.grpc.Server;
import io.grpc.Status;
import io.grpc.stub.ServerCallStreamObserver;
import io.grpc.stub.StreamObserver;
import java.io.IOException;
import java.util.List;
import java.util.Map;

/**
 * Bài 7.5 – Dịch vụ gRPC tra cứu thuật ngữ, phía server (Mã giả 7.6, Mục 7.4.3 Giáo trình Lập trình mạng).
 * Lớp GlossaryGrpc.GlossaryImplBase (GlossaryBase) do protoc-gen-grpc-java sinh từ tra_cuu.proto.
 * Kế thừa Mã giả 3.7: nhánh "lookup" của PROCESS_REQUEST thành phương thức lookup.
 * Chạy: mvn -q compile exec:java -Dexec.mainClass=GlossaryServer -Dexec.args="[host] [port] [độ_trễ_ms]"
 *       (mặc định 0.0.0.0 5001 0; Grpc.newServerBuilderForPort lắng nghe mọi địa chỉ, tham số host chỉ để in)
 */
public class GlossaryServer {
    static final Map<String, String> DICT = Map.of(
            "TCP", "Giao thức vận chuyển hướng kết nối",
            "UDP", "Giao thức vận chuyển không kết nối");
    static final List<String> CHANGES = List.of("HTTP/2", "QUIC", "HTTP/3", "gRPC", "HTTPS");   // dòng thay đổi mô phỏng

    static class GlossaryService extends GlossaryGrpc.GlossaryImplBase {     // EXTENDS GlossaryBase
        final int delayMs;
        GlossaryService(int delayMs) { this.delayMs = delayMs; }

        @Override
        public void lookup(LookupRequest req, StreamObserver<Term> obs) {
            ServerCallStreamObserver<Term> so = (ServerCallStreamObserver<Term>) obs;
            if (delayMs > 0) {
                sleep(delayMs);                                         // server cố ý chậm
                if (so.isCancelled()) {                                 // hạn chót truyền qua grpc-timeout
                    System.out.println("[server] Lookup('" + req.getTerm() + "') bị hủy: client đã hết hạn chót");
                    return;
                }
            }
            if (!DICT.containsKey(req.getTerm())) {
                System.out.println("[server] Lookup('" + req.getTerm() + "') -> NOT_FOUND");
                obs.onError(Status.NOT_FOUND.withDescription("Không có " + req.getTerm()).asRuntimeException());
                return;                                                 // RAISE RpcError(NOT_FOUND, …)
            }
            System.out.println("[server] Lookup('" + req.getTerm() + "') -> OK");
            obs.onNext(Term.newBuilder().setTerm(req.getTerm()).setText(DICT.get(req.getTerm())).build());
            obs.onCompleted();
        }

        @Override
        public void watch(WatchRequest req, StreamObserver<Term> obs) {     // luồng từ server
            ServerCallStreamObserver<Term> so = (ServerCallStreamObserver<Term>) obs;
            System.out.println("[server] Watch(prefix='" + req.getPrefix() + "') bắt đầu");
            for (String term : CHANGES) {                               // CHANGES_FEED(): mỗi 0,5 giây một thuật ngữ
                sleep(500);
                if (so.isCancelled()) return;                           // client đã hủy
                if (term.startsWith(req.getPrefix())) {
                    obs.onNext(Term.newBuilder().setTerm(term).setText("Thuật ngữ mới: " + term).addTags("moi").build());
                    System.out.println("[server] Watch: gửi " + term);
                }
            }
            obs.onCompleted();
            System.out.println("[server] Watch kết thúc");
        }
    }

    static void sleep(int ms) {
        try { Thread.sleep(ms); } catch (InterruptedException e) { Thread.currentThread().interrupt(); }
    }

    public static void main(String[] args) throws IOException, InterruptedException {
        String host = args.length >= 1 ? args[0] : "0.0.0.0";
        int port = args.length >= 2 ? Integer.parseInt(args[1]) : 5001;
        int delayMs = args.length >= 3 ? Integer.parseInt(args[2]) : 0;
        Server server = Grpc.newServerBuilderForPort(port, InsecureServerCredentials.create())   // h2c, không TLS
                .addService(new GlossaryService(delayMs)).build().start();
        System.out.println("[server] Dịch vụ gRPC glossary.v1.Glossary tại " + host + ":" + port
                + " (độ trễ Lookup " + delayMs + " ms)");
        Runtime.getRuntime().addShutdownHook(new Thread(() -> {
            server.shutdown();
            System.out.println("[server] Dừng server");
        }));
        server.awaitTermination();
    }
}

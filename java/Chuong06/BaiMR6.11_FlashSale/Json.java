import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Bộ mã hóa, giải mã JSON tối giản (JDK không có thư viện JSON chuẩn).
 * Ánh xạ: đối tượng ↔ LinkedHashMap (giữ thứ tự khóa), mảng ↔ ArrayList, số nguyên ↔ Long,
 * số thực ↔ Double, chuỗi ↔ String, true/false ↔ Boolean, null ↔ null.
 * Không thêm khoảng trắng và giữ nguyên chữ có dấu, giống bản C#, Python của giáo trình.
 */
final class Json {
    private final String s;
    private int i;

    private Json(String s) { this.s = s; }

    /** JSON_ENCODE(obj). */
    static String stringify(Object v) {
        if (v == null) return "null";
        if (v instanceof String str) return quote(str);
        if (v instanceof Number || v instanceof Boolean) return v.toString();
        StringBuilder b = new StringBuilder();
        if (v instanceof Map<?, ?> m) {
            b.append('{');
            for (Map.Entry<?, ?> e : m.entrySet()) {
                if (b.length() > 1) b.append(',');
                b.append(quote(e.getKey().toString())).append(':').append(stringify(e.getValue()));
            }
            return b.append('}').toString();
        }
        if (v instanceof List<?> list) {
            b.append('[');
            for (Object x : list) {
                if (b.length() > 1) b.append(',');
                b.append(stringify(x));
            }
            return b.append(']').toString();
        }
        throw new IllegalArgumentException("Kiểu không hỗ trợ: " + v.getClass());
    }

    private static String quote(String str) {
        StringBuilder b = new StringBuilder("\"");
        for (char c : str.toCharArray()) {
            switch (c) {
                case '"' -> b.append("\\\"");
                case '\\' -> b.append("\\\\");
                case '\n' -> b.append("\\n");
                case '\r' -> b.append("\\r");
                case '\t' -> b.append("\\t");
                default -> { if (c < 0x20) b.append(String.format("\\u%04x", (int) c)); else b.append(c); }
            }
        }
        return b.append('"').toString();
    }

    /** JSON_DECODE(text); lỗi cú pháp ném IllegalArgumentException. */
    static Object parse(String text) {
        Json p = new Json(text);
        Object v = p.value();
        p.ws();
        if (p.i != text.length()) throw new IllegalArgumentException("JSON sai: thừa ký tự");
        return v;
    }

    /** Tiện ích: tạo đối tượng JSON từ các cặp khóa, giá trị. */
    static Map<String, Object> obj(Object... kv) {
        Map<String, Object> m = new LinkedHashMap<>();
        for (int k = 0; k + 1 < kv.length; k += 2) m.put((String) kv[k], kv[k + 1]);
        return m;
    }

    private void ws() { while (i < s.length() && Character.isWhitespace(s.charAt(i))) i++; }

    private char peek() {
        ws();
        if (i >= s.length()) throw new IllegalArgumentException("JSON sai: hết dữ liệu");
        return s.charAt(i);
    }

    private void expect(char c) {
        if (peek() != c) throw new IllegalArgumentException("JSON sai: cần '" + c + "' tại vị trí " + i);
        i++;
    }

    private Object value() {
        char c = peek();
        if (c == '{') {
            Map<String, Object> m = new LinkedHashMap<>();
            i++;
            if (peek() == '}') { i++; return m; }
            do { String k = string(); expect(':'); m.put(k, value()); } while (peek() == ',' && ++i > 0);
            expect('}');
            return m;
        }
        if (c == '[') {
            List<Object> list = new ArrayList<>();
            i++;
            if (peek() == ']') { i++; return list; }
            do { list.add(value()); } while (peek() == ',' && ++i > 0);
            expect(']');
            return list;
        }
        if (c == '"') return string();
        if (s.startsWith("true", i)) { i += 4; return Boolean.TRUE; }
        if (s.startsWith("false", i)) { i += 5; return Boolean.FALSE; }
        if (s.startsWith("null", i)) { i += 4; return null; }
        int start = i;
        while (i < s.length() && "+-0123456789.eE".indexOf(s.charAt(i)) >= 0) i++;
        String num = s.substring(start, i);
        if (num.isEmpty()) throw new IllegalArgumentException("JSON sai tại vị trí " + start);
        return num.matches("-?\\d+") ? (Object) Long.parseLong(num) : (Object) Double.parseDouble(num);
    }

    private String string() {
        expect('"');
        StringBuilder b = new StringBuilder();
        while (true) {
            if (i >= s.length()) throw new IllegalArgumentException("JSON sai: chuỗi chưa đóng");
            char c = s.charAt(i++);
            if (c == '"') return b.toString();
            if (c != '\\') { b.append(c); continue; }
            char e = s.charAt(i++);
            switch (e) {
                case 'n' -> b.append('\n');
                case 'r' -> b.append('\r');
                case 't' -> b.append('\t');
                case 'b' -> b.append('\b');
                case 'f' -> b.append('\f');
                case 'u' -> { b.append((char) Integer.parseInt(s.substring(i, i + 4), 16)); i += 4; }
                default -> b.append(e);
            }
        }
    }
}

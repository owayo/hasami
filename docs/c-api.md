# C から使う

`src/ffi.rs` が C ABI の関数（`hasami_new`・`hasami_tokenize`・`hasami_last_error` など）を公開している
（`analyzer` feature に含まれる。feature は [rust-api.md](rust-api.md) の「ライブラリとして使う」）。

```c
#include "hasami.h"

HasamiAnalyzer* analyzer = hasami_new("dict/ipadic-neologd.hsd");
if (!analyzer) {
    fprintf(stderr, "load error: %s\n", hasami_last_error(NULL));
    return 1;
}

HasamiTokenList tokens = hasami_tokenize(analyzer, "東京都に住んでいる");
const char* error = hasami_last_error(analyzer);
if (error) {
    fprintf(stderr, "tokenize error: %s\n", error);
    hasami_free(analyzer);
    return 1;
}

for (uint32_t i = 0; i < tokens.len; i++) {
    printf("%s\t%s\n", tokens.tokens[i].surface, tokens.tokens[i].pos);
}

hasami_free_tokens(tokens);
hasami_free(analyzer);
```

`HasamiToken` のフィールドは `surface`・`start`・`end`・`pos`・`conj_type`・`conj_form`・`base_form`・`reading`・
`pronunciation`・`is_known`（文字列はすべて UTF-8 のヌル終端）。解析中に辞書の不正な参照を見つけたときは、
空のリストを返して `hasami_last_error` にエラーを入れる。

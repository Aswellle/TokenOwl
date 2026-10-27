/**
 * Provider brand icon mapping data, kept free of `@lobehub/icons` imports so
 * tests can assert coverage without pulling the icon package's transitive
 * JSON imports into the Vitest transform.
 *
 * Provider ids carry their brand as the prefix before the colon
 * (`openai:account-1`); the prefix selects the official brand icon.
 */

/** providerId prefix → @lobehub/icons catalog provider id. */
export const PROVIDER_PREFIXES: Record<string, string> = {
  openai: "openai",
  anthropic: "anthropic",
  claude: "claude",
  gemini: "google",
  mistral: "mistral",
  groq: "groq",
  xai: "xai",
  siliconflow: "siliconcloud",
  stepfun: "stepfun",
  doubao: "doubao",
  baichuan: "baichuan",
  google: "google",
  openrouter: "openrouter",
  deepseek: "deepseek",
  qwen: "qwen",
  kimi: "moonshot",
  moonshot: "moonshot",
  minimax: "minimax",
  zhipu: "zhipu",
  glm: "zhipu",
  grok: "xai",
  cohere: "cohere",
  perplexity: "perplexity",
  ollama: "ollama",
  siliconcloud: "siliconcloud",
  together: "together",
  fireworks: "fireworks",
  cerebras: "cerebras",
  huggingface: "huggingface",
  microsoft: "microsoft",
  azure: "azure",
  bedrock: "bedrock",
  vertexai: "vertexai",
  // Built-in manifests whose brand the icon catalog indexes under its own id.
  "360": "ai360",
  "baidu-qianfan": "baiducloud",
  iflytek: "spark",
  "tencent-hunyuan": "hunyuan",
  "xiaomi-mimo": "xiaomimimo",
  "new-api": "newapi",
  // OpenAI-shaped relays of a native brand read as that brand.
  "anthropic-openai": "anthropic",
  "gemini-openai": "gemini",
};

/**
 * Brands the icon catalog's provider lookup does not index but ships a
 * component for. `yi` renders its official color variant; `coze`'s mark is
 * monochrome by brand and renders in the ambient ink colour.
 */
export const DIRECT_BRANDS: Record<string, "yi" | "coze"> = {
  "01ai": "yi",
  coze: "coze",
};

# Code Structure Index

## src/ui/command.rs

### enum UiCommand
- Location: src/ui/command.rs:16
- Namespace: crate::ui::command
- Anchor: pub enum UiCommand {

## src/ui/import_menu.rs

### struct ImportMenuBusy
- Location: src/ui/import_menu.rs:10
- Namespace: crate::ui::import_menu
- Anchor: pub struct ImportMenuBusy {

### struct Entry
- Location: src/ui/import_menu.rs:19
- Namespace: crate::ui::import_menu
- Anchor: struct Entry {

### function show
- Location: src/ui/import_menu.rs:26
- Namespace: crate::ui::import_menu
- Anchor: pub fn show(ui: &mut egui::Ui, busy: ImportMenuBusy, commands: &mut Vec<UiCommand>) {

### function show_entry
- Location: src/ui/import_menu.rs:83
- Namespace: crate::ui::import_menu
- Anchor: fn show_entry(ui: &mut egui::Ui, entry: &Entry, commands: &mut Vec<UiCommand>) {

## src/ui/dialogs/curl_paste.rs

### enum CurlPasteDialogAction
- Location: src/ui/dialogs/curl_paste.rs:3
- Namespace: crate::ui::dialogs::curl_paste
- Anchor: pub enum CurlPasteDialogAction {

### function show
- Location: src/ui/dialogs/curl_paste.rs:9
- Namespace: crate::ui::dialogs::curl_paste
- Anchor: pub fn show(ctx: &egui::Context, curl_input: &mut String) -> CurlPasteDialogAction {

## src/http_format/multi.rs

### function parse_requests
- Location: src/http_format/multi.rs:19
- Namespace: crate::http_format::multi
- Anchor: pub fn parse_requests(text: &str) -> Result<Vec<RequestDraft>, HttpFormatError> {

### function split_chunks
- Location: src/http_format/multi.rs:40
- Namespace: crate::http_format::multi
- Anchor: fn split_chunks(text: &str) -> Vec<String> {

### function has_request_line
- Location: src/http_format/multi.rs:58
- Namespace: crate::http_format::multi
- Anchor: fn has_request_line(chunk: &str) -> bool {

## src/ui/shell.rs

### struct ShellContext
- Location: src/ui/shell.rs:17
- Namespace: crate::ui::shell
- Anchor: pub struct ShellContext<'a> {

### function show
- Location: src/ui/shell.rs:30
- Namespace: crate::ui::shell
- Anchor: pub fn show(ui: &mut egui::Ui, state: &mut AppState, ctx: &mut ShellContext<'_>) {

### function show_settings_window
- Location: src/ui/shell.rs:38
- Namespace: crate::ui::shell
- Anchor: fn show_settings_window(

## src/ui/top_bar.rs

### function show_topbar
- Location: src/ui/top_bar.rs:8
- Namespace: crate::ui::top_bar
- Anchor: pub fn show_topbar(

## src/ui/center_panel.rs

### function show_center
- Location: src/ui/center_panel.rs:8
- Namespace: crate::ui::center_panel
- Anchor: pub fn show_center(ui: &mut egui::Ui, state: &mut AppState, ctx: &mut ShellContext<'_>) {

### function show_history
- Location: src/ui/center_panel.rs:75
- Namespace: crate::ui::center_panel
- Anchor: fn show_history(ui: &mut egui::Ui, state: &mut AppState) {

## src/ui/request_panel.rs

### function show_request_editor
- Location: src/ui/request_panel.rs:10
- Namespace: crate::ui::request_panel
- Anchor: pub fn show_request_editor(

### function show_save_button
- Location: src/ui/request_panel.rs:60
- Namespace: crate::ui::request_panel
- Anchor: fn show_save_button(ui: &mut egui::Ui, commands: &mut Vec<UiCommand>, dirty: bool) {

### function show_header_row
- Location: src/ui/request_panel.rs:78
- Namespace: crate::ui::request_panel
- Anchor: fn show_header_row(

### function show_tab_strip
- Location: src/ui/request_panel.rs:235
- Namespace: crate::ui::request_panel
- Anchor: fn show_tab_strip(ui: &mut egui::Ui, state: &mut AppState) {

### function tab_count_hint
- Location: src/ui/request_panel.rs:262
- Namespace: crate::ui::request_panel
- Anchor: fn tab_count_hint(state: &AppState, tab: RequestTab) -> Option<usize> {

### function show_params_tab
- Location: src/ui/request_panel.rs:290
- Namespace: crate::ui::request_panel
- Anchor: fn show_params_tab(ui: &mut egui::Ui, state: &AppState, intents: &mut Vec<PanelIntent>) {

### function show_headers_tab
- Location: src/ui/request_panel.rs:314
- Namespace: crate::ui::request_panel
- Anchor: fn show_headers_tab(ui: &mut egui::Ui, state: &AppState, intents: &mut Vec<PanelIntent>) {

### function show_kv_editor
- Location: src/ui/request_panel.rs:332
- Namespace: crate::ui::request_panel
- Anchor: fn show_kv_editor(

### function show_auth_tab
- Location: src/ui/request_panel.rs:378
- Namespace: crate::ui::request_panel
- Anchor: fn show_auth_tab(ui: &mut egui::Ui, state: &AppState, intents: &mut Vec<PanelIntent>) {

### function show_oauth_hint
- Location: src/ui/request_panel.rs:544
- Namespace: crate::ui::request_panel
- Anchor: fn show_oauth_hint(

### function show_body_tab
- Location: src/ui/request_panel.rs:585
- Namespace: crate::ui::request_panel
- Anchor: fn show_body_tab(ui: &mut egui::Ui, state: &AppState, intents: &mut Vec<PanelIntent>) {

### function show_outlook_label
- Location: src/ui/request_panel.rs:489
- Namespace: crate::ui::request_panel
- Anchor: fn show_outlook_label(

### function humanize_remaining
- Location: src/ui/request_panel.rs:534
- Namespace: crate::ui::request_panel
- Anchor: fn humanize_remaining(seconds: i64) -> String {

## src/ui/intent.rs

### enum PanelIntent
- Location: src/ui/intent.rs:22
- Namespace: crate::ui::intent
- Anchor: pub enum PanelIntent {

## src/oauth/mod.rs

### const DATA_DIR
- Location: src/oauth/mod.rs:26
- Namespace: crate::oauth
- Anchor: pub(crate) const DATA_DIR: &str = "./data";

### const STORAGE
- Location: src/oauth/mod.rs:28
- Namespace: crate::oauth
- Anchor: static STORAGE: OnceLock<Option<FileStorage>> = OnceLock::new();

### function now_unix
- Location: src/oauth/mod.rs:30
- Namespace: crate::oauth
- Anchor: pub(crate) fn now_unix() -> i64 {

### function storage
- Location: src/oauth/mod.rs:37
- Namespace: crate::oauth
- Anchor: pub(crate) fn storage() -> Option<&'static FileStorage> {

### function token_store
- Location: src/oauth/mod.rs:50
- Namespace: crate::oauth
- Anchor: pub(crate) fn token_store() -> TokenStoreImpl {

### enum FlowKind
- Location: src/oauth/mod.rs:80
- Namespace: crate::oauth
- Anchor: pub enum FlowKind {

#### method FlowKind.as_str
- Location: src/oauth/mod.rs:87
- Namespace: crate::oauth
- Anchor: pub fn as_str(self) -> &'static str {

### struct Token
- Location: src/oauth/mod.rs:97
- Namespace: crate::oauth
- Anchor: pub struct Token {

#### method Token.is_expired
- Location: src/oauth/mod.rs:109
- Namespace: crate::oauth
- Anchor: pub fn is_expired(&self, now_unix: i64) -> bool {

#### method Token.expires_within
- Location: src/oauth/mod.rs:113
- Namespace: crate::oauth
- Anchor: pub fn expires_within(&self, now_unix: i64, seconds: i64) -> bool {

### enum OAuthError
- Location: src/oauth/mod.rs:119
- Namespace: crate::oauth
- Anchor: pub enum OAuthError {

#### method OAuthError.from
- Location: src/oauth/mod.rs:143
- Namespace: crate::oauth
- Anchor: fn from(error: serde_json::Error) -> Self {

#### method OAuthError.fmt
- Location: src/oauth/mod.rs:149
- Namespace: crate::oauth
- Anchor: fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {

### function token_store_at
- Location: src/oauth/mod.rs:66
- Namespace: crate::oauth
- Anchor: pub(crate) fn token_store_at(_base_dir: &str) -> TokenStoreImpl {

### type TokenStoreImpl
- Location: src/oauth/mod.rs:76
- Namespace: crate::oauth
- Anchor: pub(crate) type TokenStoreImpl = KeyringTokenStore;

## src/oauth/middleware.rs

### const REFRESH_BUFFER_SECONDS
- Location: src/oauth/middleware.rs:13
- Namespace: crate::oauth::middleware
- Anchor: const REFRESH_BUFFER_SECONDS: i64 = 60;

### type RefreshResult
- Location: src/oauth/middleware.rs:24
- Namespace: crate::oauth::middleware
- Anchor: type RefreshResult = Result<Option<AttachmentHeader>, OAuthError>;

### struct CachedAuth
- Location: src/oauth/middleware.rs:26
- Namespace: crate::oauth::middleware
- Anchor: struct CachedAuth {

### const AUTH_CACHE
- Location: src/oauth/middleware.rs:31
- Namespace: crate::oauth::middleware
- Anchor: static AUTH_CACHE: OnceLock<Mutex<HashMap<String, CachedAuth>>> = OnceLock::new();

### const REFRESH_RUNTIME
- Location: src/oauth/middleware.rs:36
- Namespace: crate::oauth::middleware
- Anchor: static REFRESH_RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();

### const INFLIGHT_REFRESH
- Location: src/oauth/middleware.rs:41
- Namespace: crate::oauth::middleware
- Anchor: static INFLIGHT_REFRESH: OnceLock<Mutex<HashMap<String, Vec<mpsc::Sender<RefreshResult>>>>> =

### function auth_cache
- Location: src/oauth/middleware.rs:44
- Namespace: crate::oauth::middleware
- Anchor: fn auth_cache() -> &'static Mutex<HashMap<String, CachedAuth>> {

### function inflight_refresh
- Location: src/oauth/middleware.rs:48
- Namespace: crate::oauth::middleware
- Anchor: fn inflight_refresh() -> &'static Mutex<HashMap<String, Vec<mpsc::Sender<RefreshResult>>>> {

### function refresh_runtime
- Location: src/oauth/middleware.rs:52
- Namespace: crate::oauth::middleware
- Anchor: fn refresh_runtime() -> Result<&'static tokio::runtime::Runtime, OAuthError> {

### function cache_key
- Location: src/oauth/middleware.rs:67
- Namespace: crate::oauth::middleware
- Anchor: fn cache_key(base_dir: &str, env_id: &str) -> String {

### function clone_result_for_fanout
- Location: src/oauth/middleware.rs:80
- Namespace: crate::oauth::middleware
- Anchor: fn clone_result_for_fanout(result: &RefreshResult) -> RefreshResult {

### function cache_auth
- Location: src/oauth/middleware.rs:87
- Namespace: crate::oauth::middleware
- Anchor: fn cache_auth(key: &str, header: AttachmentHeader, token: &Token) {

### function invalidate
- Location: src/oauth/middleware.rs:99
- Namespace: crate::oauth::middleware
- Anchor: pub fn invalidate(env_id: &str) {

### function invalidate_at
- Location: src/oauth/middleware.rs:103
- Namespace: crate::oauth::middleware
- Anchor: pub(crate) fn invalidate_at(env_id: &str, base_dir: &str) {

### struct AttachmentHeader
- Location: src/oauth/middleware.rs:112
- Namespace: crate::oauth::middleware
- Anchor: pub struct AttachmentHeader {

### enum AuthResolution
- Location: src/oauth/middleware.rs:117
- Namespace: crate::oauth::middleware
- Anchor: pub enum AuthResolution {

### function resolve_authorization
- Location: src/oauth/middleware.rs:132
- Namespace: crate::oauth::middleware
- Anchor: pub fn resolve_authorization(env_name: &str) -> AuthResolution {

### function resolve_authorization_at
- Location: src/oauth/middleware.rs:136
- Namespace: crate::oauth::middleware
- Anchor: pub(crate) fn resolve_authorization_at(env_name: &str, base_dir: &str) -> AuthResolution {

### function attachment_for
- Location: src/oauth/middleware.rs:390
- Namespace: crate::oauth::middleware
- Anchor: fn attachment_for(config: &OAuthConfig, token: &Token) -> AttachmentHeader {

### function block_on_refresh
- Location: src/oauth/middleware.rs:397
- Namespace: crate::oauth::middleware
- Anchor: fn block_on_refresh(

### function renewal_lead_seconds
- Location: src/oauth/middleware.rs:19
- Namespace: crate::oauth::middleware
- Anchor: fn renewal_lead_seconds(token: &Token) -> i64 {

### enum TokenOutlook
- Location: src/oauth/middleware.rs:221
- Namespace: crate::oauth::middleware
- Anchor: pub enum TokenOutlook {

### function token_outlook
- Location: src/oauth/middleware.rs:236
- Namespace: crate::oauth::middleware
- Anchor: pub fn token_outlook(env_name: &str) -> TokenOutlook {

### function token_outlook_at
- Location: src/oauth/middleware.rs:240
- Namespace: crate::oauth::middleware
- Anchor: pub(crate) fn token_outlook_at(env_name: &str, base_dir: &str) -> TokenOutlook {

### enum TokenPlan
- Location: src/oauth/middleware.rs:277
- Namespace: crate::oauth::middleware
- Anchor: enum TokenPlan {

### function plan_for_token
- Location: src/oauth/middleware.rs:289
- Namespace: crate::oauth::middleware
- Anchor: fn plan_for_token(token: &Token, flow: FlowKind, config: &OAuthConfig, now: i64) -> TokenPlan {

### struct RenewalContext
- Location: src/oauth/middleware.rs:320
- Namespace: crate::oauth::middleware
- Anchor: struct RenewalContext {

### function start_renewal
- Location: src/oauth/middleware.rs:335
- Namespace: crate::oauth::middleware
- Anchor: fn start_renewal<F>(ctx: RenewalContext, acquire: F) -> AuthResolution

### function block_on_client_credentials
- Location: src/oauth/middleware.rs:405
- Namespace: crate::oauth::middleware
- Anchor: fn block_on_client_credentials(config: &ClientCredentialsConfig) -> Result<Token, OAuthError> {

## src/oauth/store.rs

### const INTERNAL_DIR
- Location: src/oauth/store.rs:16
- Namespace: crate::oauth::store
- Anchor: const INTERNAL_DIR: &str = ".probe";

### const TOKENS_DIR
- Location: src/oauth/store.rs:18
- Namespace: crate::oauth::store
- Anchor: const TOKENS_DIR: &str = "oauth_tokens";

### trait TokenStore
- Location: src/oauth/store.rs:20
- Namespace: crate::oauth::store
- Anchor: pub trait TokenStore {

#### method TokenStore.get
- Location: src/oauth/store.rs:21
- Namespace: crate::oauth::store
- Anchor: fn get(&self, env_id: &str, flow_id: &str) -> Result<Option<Token>, OAuthError>;

#### method TokenStore.put
- Location: src/oauth/store.rs:22
- Namespace: crate::oauth::store
- Anchor: fn put(&self, env_id: &str, flow_id: &str, token: &Token) -> Result<(), OAuthError>;

#### method TokenStore.delete
- Location: src/oauth/store.rs:23
- Namespace: crate::oauth::store
- Anchor: fn delete(&self, env_id: &str, flow_id: &str) -> Result<(), OAuthError>;

### struct EnvTokenFile
- Location: src/oauth/store.rs:27
- Namespace: crate::oauth::store
- Anchor: struct EnvTokenFile {

### struct FileTokenStore
- Location: src/oauth/store.rs:36
- Namespace: crate::oauth::store
- Anchor: pub struct FileTokenStore {

#### method FileTokenStore.new
- Location: src/oauth/store.rs:42
- Namespace: crate::oauth::store
- Anchor: pub fn new<P: AsRef<Path>>(base_dir: P) -> Self {

#### method FileTokenStore.tokens_dir
- Location: src/oauth/store.rs:48
- Namespace: crate::oauth::store
- Anchor: fn tokens_dir(&self) -> PathBuf {

#### method FileTokenStore.env_path
- Location: src/oauth/store.rs:52
- Namespace: crate::oauth::store
- Anchor: fn env_path(&self, env_id: &str) -> Result<PathBuf, OAuthError> {

#### method FileTokenStore.load_env
- Location: src/oauth/store.rs:57
- Namespace: crate::oauth::store
- Anchor: fn load_env(&self, env_id: &str) -> Result<EnvTokenFile, OAuthError> {

#### method FileTokenStore.save_env
- Location: src/oauth/store.rs:69
- Namespace: crate::oauth::store
- Anchor: fn save_env(&self, env_id: &str, file: &EnvTokenFile) -> Result<(), OAuthError> {

#### method FileTokenStore.get
- Location: src/oauth/store.rs:94
- Namespace: crate::oauth::store
- Anchor: fn get(&self, env_id: &str, flow_id: &str) -> Result<Option<Token>, OAuthError> {

#### method FileTokenStore.put
- Location: src/oauth/store.rs:100
- Namespace: crate::oauth::store
- Anchor: fn put(&self, env_id: &str, flow_id: &str, token: &Token) -> Result<(), OAuthError> {

#### method FileTokenStore.delete
- Location: src/oauth/store.rs:110
- Namespace: crate::oauth::store
- Anchor: fn delete(&self, env_id: &str, flow_id: &str) -> Result<(), OAuthError> {

### struct KeyringTokenStore
- Location: src/oauth/store.rs:164
- Namespace: crate::oauth::store
- Anchor: pub struct KeyringTokenStore;

#### method KeyringTokenStore.entry
- Location: src/oauth/store.rs:170
- Namespace: crate::oauth::store
- Anchor: fn entry(env_id: &str) -> Result<keyring::Entry, OAuthError> {

#### method KeyringTokenStore.load_env
- Location: src/oauth/store.rs:175
- Namespace: crate::oauth::store
- Anchor: fn load_env(env_id: &str) -> Result<EnvTokenFile, OAuthError> {

#### method KeyringTokenStore.save_env
- Location: src/oauth/store.rs:184
- Namespace: crate::oauth::store
- Anchor: fn save_env(env_id: &str, file: &EnvTokenFile) -> Result<(), OAuthError> {

#### method KeyringTokenStore.get
- Location: src/oauth/store.rs:201
- Namespace: crate::oauth::store
- Anchor: fn get(&self, env_id: &str, flow_id: &str) -> Result<Option<Token>, OAuthError> {

#### method KeyringTokenStore.put
- Location: src/oauth/store.rs:208
- Namespace: crate::oauth::store
- Anchor: fn put(&self, env_id: &str, flow_id: &str, token: &Token) -> Result<(), OAuthError> {

#### method KeyringTokenStore.delete
- Location: src/oauth/store.rs:218
- Namespace: crate::oauth::store
- Anchor: fn delete(&self, env_id: &str, flow_id: &str) -> Result<(), OAuthError> {

### function write_lock
- Location: src/oauth/store.rs:241
- Namespace: crate::oauth::store
- Anchor: fn write_lock(key: &str) -> Arc<Mutex<()>> {

### function env_lock_key
- Location: src/oauth/store.rs:256
- Namespace: crate::oauth::store
- Anchor: fn env_lock_key(path: &Path) -> String {

### function validate_key
- Location: src/oauth/store.rs:267
- Namespace: crate::oauth::store
- Anchor: fn validate_key(key: &str) -> Result<(), OAuthError> {

### function atomic_write
- Location: src/oauth/store.rs:283
- Namespace: crate::oauth::store
- Anchor: fn atomic_write(path: &Path, data: &[u8]) -> Result<(), OAuthError> {

### function create_token_tmp_file
- Location: src/oauth/store.rs:329
- Namespace: crate::oauth::store
- Anchor: fn create_token_tmp_file(path: &Path) -> io::Result<fs::File> {

### function unique_tmp_path
- Location: src/oauth/store.rs:341
- Namespace: crate::oauth::store
- Anchor: fn unique_tmp_path(path: &Path) -> PathBuf {

## src/oauth/flows/mod.rs

### function build_basic_client_with_token_only
- Location: src/oauth/flows/mod.rs:14
- Namespace: crate::oauth::flows
- Anchor: pub(crate) fn build_basic_client_with_token_only(

### function collect_extra_params
- Location: src/oauth/flows/mod.rs:29
- Namespace: crate::oauth::flows
- Anchor: pub(crate) fn collect_extra_params(

### const DEFAULT_TOKEN_LIFETIME_SECONDS
- Location: src/oauth/flows/mod.rs:58
- Namespace: crate::oauth::flows
- Anchor: const DEFAULT_TOKEN_LIFETIME_SECONDS: i64 = 3600;

### function build_cached_token
- Location: src/oauth/flows/mod.rs:60
- Namespace: crate::oauth::flows
- Anchor: pub(crate) fn build_cached_token<R: TokenResponse<BasicTokenType>>(

## src/oauth/flows/refresh.rs

### struct RefreshConfig
- Location: src/oauth/flows/refresh.rs:9
- Namespace: crate::oauth::flows::refresh
- Anchor: pub struct RefreshConfig {

### function run
- Location: src/oauth/flows/refresh.rs:16
- Namespace: crate::oauth::flows::refresh
- Anchor: pub async fn run(

## src/oauth/flows/client_credentials.rs

### struct ClientCredentialsConfig
- Location: src/oauth/flows/client_credentials.rs:9
- Namespace: crate::oauth::flows::client_credentials
- Anchor: pub struct ClientCredentialsConfig {

### function run
- Location: src/oauth/flows/client_credentials.rs:19
- Namespace: crate::oauth::flows::client_credentials
- Anchor: pub async fn run(config: &ClientCredentialsConfig) -> Result<Token, OAuthError> {

## src/oauth/flows/auth_code.rs

### struct AuthCodeConfig
- Location: src/oauth/flows/auth_code.rs:14
- Namespace: crate::oauth::flows::auth_code
- Anchor: pub struct AuthCodeConfig {

### function run
- Location: src/oauth/flows/auth_code.rs:25
- Namespace: crate::oauth::flows::auth_code
- Anchor: pub async fn run(config: &AuthCodeConfig) -> Result<Token, OAuthError> {

### function build_client
- Location: src/oauth/flows/auth_code.rs:86
- Namespace: crate::oauth::flows::auth_code
- Anchor: fn build_client(config: &AuthCodeConfig, redirect_uri: &str) -> Result<BasicClient, OAuthError> {

## src/oauth/flows/device_code.rs

### struct DeviceCodeConfig
- Location: src/oauth/flows/device_code.rs:10
- Namespace: crate::oauth::flows::device_code
- Anchor: pub struct DeviceCodeConfig {

### enum DeviceCodeEvent
- Location: src/oauth/flows/device_code.rs:22
- Namespace: crate::oauth::flows::device_code
- Anchor: pub enum DeviceCodeEvent {

### function run
- Location: src/oauth/flows/device_code.rs:32
- Namespace: crate::oauth::flows::device_code
- Anchor: pub async fn run<F>(config: &DeviceCodeConfig, on_event: F)

### function build_client
- Location: src/oauth/flows/device_code.rs:100
- Namespace: crate::oauth::flows::device_code
- Anchor: fn build_client(config: &DeviceCodeConfig) -> Result<BasicClient, OAuthError> {

## src/ui/oauth_panel.rs

### enum FlowEvent
- Location: src/ui/oauth_panel.rs:15
- Namespace: crate::ui::oauth_panel
- Anchor: enum FlowEvent {

### struct DeviceVerification
- Location: src/ui/oauth_panel.rs:26
- Namespace: crate::ui::oauth_panel
- Anchor: struct DeviceVerification {

### struct OAuthPanelState
- Location: src/ui/oauth_panel.rs:34
- Namespace: crate::ui::oauth_panel
- Anchor: pub struct OAuthPanelState {

#### method OAuthPanelState.default
- Location: src/ui/oauth_panel.rs:47
- Namespace: crate::ui::oauth_panel
- Anchor: fn default() -> Self {

### function flow_index
- Location: src/ui/oauth_panel.rs:62
- Namespace: crate::ui::oauth_panel
- Anchor: fn flow_index(flow: FlowKind) -> usize {

### function show
- Location: src/ui/oauth_panel.rs:70
- Namespace: crate::ui::oauth_panel
- Anchor: pub fn show(ui: &mut egui::Ui, panel: &mut OAuthPanelState, env_name: Option<&str>) {

### function sync_if_env_changed
- Location: src/ui/oauth_panel.rs:105
- Namespace: crate::ui::oauth_panel
- Anchor: fn sync_if_env_changed(panel: &mut OAuthPanelState, env_name: &str, env_id: &str) {

### function persist_if_changed
- Location: src/ui/oauth_panel.rs:135
- Namespace: crate::ui::oauth_panel
- Anchor: fn persist_if_changed(panel: &mut OAuthPanelState) {

### function poll_flow_events
- Location: src/ui/oauth_panel.rs:158
- Namespace: crate::ui::oauth_panel
- Anchor: fn poll_flow_events(panel: &mut OAuthPanelState) {

### function render_flow_selector
- Location: src/ui/oauth_panel.rs:215
- Namespace: crate::ui::oauth_panel
- Anchor: fn render_flow_selector(ui: &mut egui::Ui, panel: &mut OAuthPanelState) {

### function render_auth_code_section
- Location: src/ui/oauth_panel.rs:246
- Namespace: crate::ui::oauth_panel
- Anchor: fn render_auth_code_section(ui: &mut egui::Ui, panel: &mut OAuthPanelState) {

### function render_client_credentials_section
- Location: src/ui/oauth_panel.rs:315
- Namespace: crate::ui::oauth_panel
- Anchor: fn render_client_credentials_section(ui: &mut egui::Ui, panel: &mut OAuthPanelState) {

### function render_device_code_section
- Location: src/ui/oauth_panel.rs:376
- Namespace: crate::ui::oauth_panel
- Anchor: fn render_device_code_section(ui: &mut egui::Ui, panel: &mut OAuthPanelState) {

### function render_device_verification
- Location: src/ui/oauth_panel.rs:450
- Namespace: crate::ui::oauth_panel
- Anchor: fn render_device_verification(ui: &mut egui::Ui, verification: &DeviceVerification) {

### function render_action_row
- Location: src/ui/oauth_panel.rs:484
- Namespace: crate::ui::oauth_panel
- Anchor: fn render_action_row(

### function reset_token
- Location: src/ui/oauth_panel.rs:520
- Namespace: crate::ui::oauth_panel
- Anchor: fn reset_token(panel: &mut OAuthPanelState, flow: FlowKind) {

### function render_token_pill
- Location: src/ui/oauth_panel.rs:531
- Namespace: crate::ui::oauth_panel
- Anchor: fn render_token_pill(ui: &mut egui::Ui, token: Option<&Token>, in_flight: bool) {

### function render_injection_section
- Location: src/ui/oauth_panel.rs:558
- Namespace: crate::ui::oauth_panel
- Anchor: fn render_injection_section(ui: &mut egui::Ui, panel: &mut OAuthPanelState) {

### function extra_params_editor
- Location: src/ui/oauth_panel.rs:574
- Namespace: crate::ui::oauth_panel
- Anchor: fn extra_params_editor(ui: &mut egui::Ui, params: &mut Vec<(String, String)>) {

### function humanize_duration
- Location: src/ui/oauth_panel.rs:603
- Namespace: crate::ui::oauth_panel
- Anchor: fn humanize_duration(seconds: i64) -> String {

### function single_line
- Location: src/ui/oauth_panel.rs:613
- Namespace: crate::ui::oauth_panel
- Anchor: fn single_line(ui: &mut egui::Ui, label: &str, buffer: &mut String, hint: &str) {

### function spawn_flow
- Location: src/ui/oauth_panel.rs:624
- Namespace: crate::ui::oauth_panel
- Anchor: fn spawn_flow<F, Fut>(make_future: F) -> mpsc::Receiver<FlowEvent>

### function spawn_auth_code_flow
- Location: src/ui/oauth_panel.rs:646
- Namespace: crate::ui::oauth_panel
- Anchor: fn spawn_auth_code_flow(config: AuthCodeConfig) -> mpsc::Receiver<FlowEvent> {

### function spawn_client_credentials_flow
- Location: src/ui/oauth_panel.rs:656
- Namespace: crate::ui::oauth_panel
- Anchor: fn spawn_client_credentials_flow(config: ClientCredentialsConfig) -> mpsc::Receiver<FlowEvent> {

### function spawn_device_code_flow
- Location: src/ui/oauth_panel.rs:666
- Namespace: crate::ui::oauth_panel
- Anchor: fn spawn_device_code_flow(config: DeviceCodeConfig) -> mpsc::Receiver<FlowEvent> {

## src/app.rs

### struct PendingRequestContext
- Location: src/app.rs:29
- Namespace: crate::app
- Anchor: struct PendingRequestContext {

### struct PendingOAuthAuth
- Location: src/app.rs:36
- Namespace: crate::app
- Anchor: struct PendingOAuthAuth {

### struct PendingRequestPreview
- Location: src/app.rs:45
- Namespace: crate::app
- Anchor: struct PendingRequestPreview {

### struct ProbeApp
- Location: src/app.rs:51
- Namespace: crate::app
- Anchor: pub struct ProbeApp {

#### method ProbeApp.new
- Location: src/app.rs:89
- Namespace: crate::app
- Anchor: pub fn new() -> Self {

#### method ProbeApp.export_workspace
- Location: src/app.rs:204
- Namespace: crate::app
- Anchor: fn export_workspace(&mut self) {

#### method ProbeApp.import_workspace
- Location: src/app.rs:235
- Namespace: crate::app
- Anchor: fn import_workspace(&mut self) {

#### method ProbeApp.confirm_workspace_import
- Location: src/app.rs:279
- Namespace: crate::app
- Anchor: fn confirm_workspace_import(&mut self) {

#### method ProbeApp.show_import_confirmation
- Location: src/app.rs:317
- Namespace: crate::app
- Anchor: fn show_import_confirmation(&mut self, ctx: &egui::Context) {

#### method ProbeApp.import_openapi_file
- Location: src/app.rs:333
- Namespace: crate::app
- Anchor: fn import_openapi_file(&mut self) {

#### method ProbeApp.import_http_file
- Location: src/app.rs:355
- Namespace: crate::app
- Anchor: fn import_http_file(&mut self) {

#### method ProbeApp.import_openapi_from_url
- Location: src/app.rs:392
- Namespace: crate::app
- Anchor: fn import_openapi_from_url(&mut self) {

#### method ProbeApp.apply_openapi_text
- Location: src/app.rs:402
- Namespace: crate::app
- Anchor: fn apply_openapi_text(&mut self, text: &str, source: String) {

#### method ProbeApp.confirm_openapi_import
- Location: src/app.rs:423
- Namespace: crate::app
- Anchor: fn confirm_openapi_import(&mut self) {

#### method ProbeApp.show_openapi_import_confirmation
- Location: src/app.rs:440
- Namespace: crate::app
- Anchor: fn show_openapi_import_confirmation(&mut self, ctx: &egui::Context) {

#### method ProbeApp.show_openapi_url_dialog
- Location: src/app.rs:456
- Namespace: crate::app
- Anchor: fn show_openapi_url_dialog(&mut self, ctx: &egui::Context) {

#### method ProbeApp.can_start_request_preview
- Location: src/app.rs:471
- Namespace: crate::app
- Anchor: fn can_start_request_preview(&self) -> bool {

#### method ProbeApp.preview_selected_request
- Location: src/app.rs:478
- Namespace: crate::app
- Anchor: fn preview_selected_request(&mut self) {

#### method ProbeApp.preview_request_at_index
- Location: src/app.rs:486
- Namespace: crate::app
- Anchor: fn preview_request_at_index(&mut self, request_index: usize) {

#### method ProbeApp.handle_pending_ui_actions
- Location: src/app.rs:582
- Namespace: crate::app
- Anchor: fn handle_pending_ui_actions(&mut self) {

#### method ProbeApp.submit_previewed_request
- Location: src/app.rs:594
- Namespace: crate::app
- Anchor: fn submit_previewed_request(&mut self) {

#### method ProbeApp.show_request_preview
- Location: src/app.rs:625
- Namespace: crate::app
- Anchor: fn show_request_preview(&mut self, ctx: &egui::Context) {

#### method ProbeApp.save_snapshot
- Location: src/app.rs:641
- Namespace: crate::app
- Anchor: fn save_snapshot(&mut self) {

#### method ProbeApp.apply_pending_intents
- Location: src/app.rs:658
- Namespace: crate::app
- Anchor: fn apply_pending_intents(&mut self) {

#### method ProbeApp.apply_pending_commands
- Location: src/app.rs:667
- Namespace: crate::app
- Anchor: fn apply_pending_commands(&mut self) {

#### method ProbeApp.apply_command
- Location: src/app.rs:674
- Namespace: crate::app
- Anchor: fn apply_command(&mut self, command: UiCommand) {

#### method ProbeApp.import_menu_busy
- Location: src/app.rs:696
- Namespace: crate::app
- Anchor: fn import_menu_busy(&self) -> ImportMenuBusy {

#### method ProbeApp.show_curl_paste_dialog
- Location: src/app.rs:703
- Namespace: crate::app
- Anchor: fn show_curl_paste_dialog(&mut self, ctx: &egui::Context) {

#### method ProbeApp.apply_intent
- Location: src/app.rs:723
- Namespace: crate::app
- Anchor: fn apply_intent(&mut self, intent: PanelIntent) {

#### method ProbeApp.has_unsaved_changes
- Location: src/app.rs:751
- Namespace: crate::app
- Anchor: fn has_unsaved_changes(&self) -> bool {

#### method ProbeApp.show_unsaved_changes_dialog
- Location: src/app.rs:758
- Namespace: crate::app
- Anchor: fn show_unsaved_changes_dialog(&mut self, ctx: &egui::Context) {

#### method ProbeApp.poll_pending_openapi_fetch
- Location: src/app.rs:783
- Namespace: crate::app
- Anchor: fn poll_pending_openapi_fetch(&mut self) {

#### method ProbeApp.poll_pending_oauth_auth
- Location: src/app.rs:803
- Namespace: crate::app
- Anchor: fn poll_pending_oauth_auth(&mut self, ctx: &egui::Context) {

#### method ProbeApp.default
- Location: src/app.rs:864
- Namespace: crate::app
- Anchor: fn default() -> Self {

#### method ProbeApp.ui
- Location: src/app.rs:870
- Namespace: crate::app
- Anchor: fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {

### function apply_intent_to_state
- Location: src/app.rs:1059
- Namespace: crate::app
- Anchor: fn apply_intent_to_state(state: &mut AppState, intent: PanelIntent) {

### function next_auto_environment_name
- Location: src/app.rs:1205
- Namespace: crate::app
- Anchor: fn next_auto_environment_name(state: &AppState) -> String {

### function apply_pending_request_context
- Location: src/app.rs:1216
- Namespace: crate::app
- Anchor: fn apply_pending_request_context(

### function create_storage
- Location: src/app.rs:1230
- Namespace: crate::app
- Anchor: fn create_storage() -> Option<FileStorage> {

## src/oauth/jwt_exp.rs

### function exp_claim
- Location: src/oauth/jwt_exp.rs:20
- Namespace: crate::oauth::jwt_exp
- Anchor: pub(crate) fn exp_claim(access_token: &str) -> Option<i64> {

### function jwt_payload
- Location: src/oauth/jwt_exp.rs:33
- Namespace: crate::oauth::jwt_exp
- Anchor: fn jwt_payload(access_token: &str) -> Option<Vec<u8>> {

## src/oauth/reacquire.rs

### function client_credentials_config
- Location: src/oauth/reacquire.rs:19
- Namespace: crate::oauth::reacquire
- Anchor: pub(crate) fn client_credentials_config(config: &OAuthConfig) -> Option<ClientCredentialsConfig> {

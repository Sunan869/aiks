# DingTalk read-only wire contracts

Checked 2026-09-23. These are upstream integration contracts, not a claim that a real company has authorized or tested this application. The adapter is constructed only with validated settings and a server-resolved secret. No production config can replace its API origins.

| Purpose | Method and fixed endpoint | Input / required response |
| --- | --- | --- |
| Browser authorization | GET https://login.dingtalk.com/oauth2/auth | client_id, redirect_uri, response_type=code, scope=openid corpid, prompt=consent, one-time state; callback uses authCode/state |
| User token | POST https://api.dingtalk.com/v1.0/oauth2/userAccessToken | camelCase clientId/clientSecret/code/grantType; require accessToken and matching corpId |
| Authorized user identity | GET https://api.dingtalk.com/v1.0/contact/users/me | x-acs-dingtalk-access-token is the USER token; require unionId |
| Company app token | POST https://api.dingtalk.com/v1.0/oauth2/{corpId}/token | snake_case client_id/client_secret/grant_type; access_token and expires_in; cached singleflight until 60 seconds before expiry |
| Map organization employee | POST https://oapi.dingtalk.com/topapi/user/getbyunionid | app access_token query; unionid JSON; require result.userid and employee contact_type=0 |
| Confirm employee | POST https://oapi.dingtalk.com/topapi/v2/user/get | userid, language=zh_CN; require userid/unionid/name/active, matching the authorized identity |
| Scope root metadata | POST https://oapi.dingtalk.com/topapi/v2/department/get | dept_id; verify returned dept_id and name |
| Direct children | POST https://oapi.dingtalk.com/topapi/v2/department/listsub | dept_id; result child dept_id,parent_id,name; never assume grandchildren included |
| Direct department members | POST https://oapi.dingtalk.com/topapi/v2/user/list | dept_id,cursor,size=100,language,contain_access_limit=false; list,has_more,next_cursor; start cursor0, stop only at has_more=false |

Minimum permission directions are user-delegated Contact.User.Read for the authorized profile, and application read access for member information, department information and department membership. The current official pages label these 成员信息读权限、通讯录部门信息读权限、通讯录部门成员读权限. Do not request contact write, phone/email, chat history, robot or approval permissions. Confirm the identifiers shown by the actual API permission picker rather than copying a broad legacy manage-addresslist permission. A personal user grant does not authorize the company directory.

Official references:
- https://open-dingtalk.github.io/developerpedia/docs/develop/permission/token/browser/get_user_app_token_browser/
- https://open-dingtalk.github.io/developerpedia/docs/develop/permission/single_to_multi/new_get_app_token/
- https://open.dingtalk.com/document/development/query-a-user-by-the-union-id
- https://open.dingtalk.com/document/development/query-user-details
- https://open.dingtalk.com/document/app/query-department-details0-v2
- https://open.dingtalk.com/document/development/user-management-acquires-the-list-departments
- https://open.dingtalk.com/document/orgapp-server/queries-the-complete-information-of-a-department-user

The open.dingtalk.com pages are JS-rendered; indexed official text confirmed methods, permission labels and pagination. Exact console permission codes still require the deployment administrator's API picker. This code deliberately rejects missing or incompatible response fields instead of guessing an active employee or a complete organization. Actual enterprise permission/application-scope behavior remains a separate acceptance check.

AIKS limits (not upstream guarantees): every HTTP request has a 10-second budget and a 2-MiB response limit, authorization flow 45 seconds and directory scan 120 seconds. HTTP 429 allows at most three attempts, numeric Retry-After at most10 seconds; a longer/date/malformed Retry-After stops the request rather than retrying too early. Other failures return fixed safe errors. No redirects, ambient proxies or response/body/URL logging. No user refresh token is persisted.

Directory collection is read-only and bounded to explicitly configured, disjoint scope roots, 10000 departments, 100000 users, one million memberships and depth128. Overlapping roots, cycles, repeated pagination, inconsistent duplicate members and missing identity/status fields fail closed. A missing active/union field in a list item is resolved through the authorized employee-detail API, never defaulted. Only complete successful input can be published atomically by TeamStore. No scope or hidden-member omission is described as the complete company outside the authorized configured range.

RED a7417d3 / Actions35835405733: the real-wire unit target fails for missing DingTalkClient, with Core regression still passing. Unit fixtures use real Axum sockets and the same production reqwest transport; the origin override exists only under cfg(test), not a shipped option. Canonical GREEN is required after implementation/formatting before this task is called complete.

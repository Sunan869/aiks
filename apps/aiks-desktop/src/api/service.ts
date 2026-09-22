export type InvokeService = (command: string, args?: Record<string, unknown>) => Promise<unknown>;
export function statusText(_state: {receiptState?:string;jobState?:string}):string { return ""; }
export class ServiceApi {
  constructor(private readonly invoke: InvokeService) {}
  async search(_query:string):Promise<{hits:unknown[]}> { await this.invoke("not_implemented"); return {hits:[]}; }
}

// Typed app SDK surface for custom TypeScript apps.
//
// An app is one ES module that default-exports an object with a handle
// function. The platform calls handle for every routed request. The
// businex global is provided by the platform runtime and is the only way
// an app touches company data: apps never see database connections,
// credentials or the host filesystem.

declare namespace businex {
  type FieldValue = string | number | boolean;

  interface RecordData {
    [field: string]: FieldValue;
  }

  interface StoredRecord {
    id: string;
    entity: string;
    data: RecordData;
  }

  interface Request {
    method: string;
    path: string;
    query: { [key: string]: string };
    body?: unknown;
  }

  interface Response {
    status: number;
    body?: unknown;
  }

  interface RecordsApi {
    list(entity: string): Promise<StoredRecord[]>;
    get(entity: string, id: string): Promise<StoredRecord>;
    create(entity: string, data: RecordData): Promise<StoredRecord>;
    update(entity: string, id: string, data: RecordData): Promise<StoredRecord>;
    remove(entity: string, id: string): Promise<void>;
  }

  interface Businex {
    records: RecordsApi;
    log(message: string, fields?: RecordData): void;
  }
}

declare interface App {
  handle(request: businex.Request): Promise<businex.Response>;
}

declare const businex: businex.Businex;

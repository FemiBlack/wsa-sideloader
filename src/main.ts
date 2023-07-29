// Import statements
import "office-ui-fabric-core/dist/css/fabric.min.css";
import {
  provideFluentDesignSystem,
  fluentButton,
  fluentCard,
  fluentProgressRing,
  baseLayerLuminance,
  StandardLuminance,
  fluentDivider,
  fluentTextField,
  fluentTooltip,
  fluentTabPanel,
  fluentTabs,
  fluentTab,
} from "@fluentui/web-components";
import { open } from "@tauri-apps/api/dialog";
import { invoke } from "@tauri-apps/api/tauri";
import { appWindow, Theme } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { Store } from "tauri-plugin-store-api";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/api/notification";

// Initialize Fluent Design System
provideFluentDesignSystem().register(
  fluentButton(),
  fluentCard(),
  fluentProgressRing(),
  fluentDivider(),
  fluentTextField(),
  fluentTooltip(),
  fluentTab(),
  fluentTabPanel(),
  fluentTabs()
);

// Interface and Types
interface PackageInfo {
  name: string;
  versionName: string;
  versionCode: string;
  label: string;
}

// Constants and Variables
let selectedFiles: string[] | null;
let chosenApkEl: HTMLElement | null;
let error: HTMLElement | null;

// Utility Functions
const Uint8ArrayToString = (array: number[]) => {
  const bufferArray = new Uint8Array(array);
  return new TextDecoder().decode(bufferArray);
};

const getFileInfo = (path: string) => {
  const fileURL = new URL(path);
  const fileName = fileURL.pathname.split("/").pop() as string;
  const fileExtension = fileName.split(".").pop();

  return {
    fileName,
    fileExtension,
  };
};

const normalizedPath = (path: string) => {
  return path.replace(/\\/g, "/");
};

const validateAddress = (address: string) => {
  return address.trim().length > 0;
};

// Store Instance
const store = new Store(".settings.dat");

// Main Functions
const sendAppNotification = async (title: string, body: string) => {
  let permissionGranted = await isPermissionGranted();
  if (!permissionGranted) {
    const permission = await requestPermission();
    permissionGranted = permission === "granted";
  }
  if (permissionGranted) {
    sendNotification({ title, body });
  }
};

const setHostAddress = async (hostAddress: string) => {
  await store.set("host-address", hostAddress);
  await store.save();
};

const setDefaultAppDir = async (path: string) => {
  await store.set("default-app-dir", path);
  await store.save();
};

const setSelectedFiles = (payload: string[] | null) => {
  selectedFiles = payload;
  if (selectedFiles && selectedFiles.length) {
    removeAllBtn.disabled = false;
    installAllBtn.disabled = false;
  } else {
    removeAllBtn.disabled = true;
    installAllBtn.disabled = true;
  }
};

const fileHandler = (name: string, type?: string, path?: string) => {
  const errorEl = error as HTMLElement;

  if (type !== "application/vnd.android.package-archive" && type !== "apk") {
    errorEl.innerText = "Please upload an APK file";
    return false;
  }

  const tabsPanel = document.querySelector("fluent-tabs") as any;

  tabsPanel!.activeid = "selectedApps";
  errorEl.innerText = "";
  const listElement = document.createElement("li");
  const pElement = document.createElement("p");
  const installButtonElement = document.createElement("fluent-button");
  const removeButtonElement = document.createElement("fluent-button") as any;
  // const fileName = new URL(apkPath).pathname.split("/").pop() as string;
  pElement.textContent = decodeURIComponent(name);
  installButtonElement.textContent = "Install";
  removeButtonElement.textContent = "Remove";
  removeButtonElement.appearance = "stealth";
  removeButtonElement.addEventListener("click", () => {
    if (!selectedFiles) return;
    setSelectedFiles(
      selectedFiles.filter(
        (p) => normalizedPath(p) === normalizedPath(path || "")
      )
    );
    console.log(selectedFiles);
    removeButtonElement.parentElement.remove();
  });
  installButtonElement.addEventListener("click", async () => {
    await runInstallation(path as string);
  });

  listElement.appendChild(pElement);
  listElement.appendChild(installButtonElement);
  listElement.appendChild(removeButtonElement);
  chosenApkEl?.append(listElement);
};

const runInstallation = async (path: string) => {
  const resp = document.getElementById("resp-msg");
  resp!.innerHTML += "<p>Processing...</p>";
  // Connect to WSA
  try {
    const con: number[] = await invoke("connect_adb");
    resp!.innerHTML += con.length
      ? "<p>" + Uint8ArrayToString(con) + "</p>"
      : "";
    await setConnectedStatus();
    // sendAppNotification(
    //   "Connection Successful",
    //   "Connected to host successfully"
    // );
  } catch (err) {
    const error = err as string;
    console.log(error);
    resp!.innerHTML += error ? "<p style='color: red'>" + error + "</p>" : "";
    // sendAppNotification(
    //   "Connection Failed",
    //   `Couldn't connect to host address: ${error}`
    // );
  }
  // Install apk
  try {
    const res: number[] = await invoke("install_application", {
      path,
    });
    resp!.innerHTML += res.length
      ? "<p>" + Uint8ArrayToString(res) + "</p>"
      : "";
    displayInstalledApps();
    sendAppNotification(
      "App Install Successfully",
      "Your app install was successful"
    );
  } catch (err) {
    console.log("Install err", err);
    resp!.innerHTML += err ? "<p style='color: red'>" + err + "</p>" : "";

    sendAppNotification("App Install Failed", `Reason: ${err}`);
  }
};

const displayApps = async () => {
  const appDir = await store.get("default-app-dir");
  const appUl = document.querySelector("#all-apps");
  appUl!.innerHTML = "";
  if (!appDir) {
    appUl!.innerHTML = "<li>App Directory not set yet</li>";
    return;
  }
  const apkList: string[] = await invoke("list_apk_files", {
    path: appDir,
  });

  if (apkList.length) {
    for (const apkPath of apkList) {
      const listElement = document.createElement("li");
      const pElement = document.createElement("p");
      const buttonElement = document.createElement("fluent-button");
      const fileName = getFileInfo(apkPath).fileName;
      pElement.textContent = decodeURIComponent(fileName);
      buttonElement.textContent = "Install";
      buttonElement.addEventListener("click", async () => {
        await runInstallation(apkPath);
      });

      listElement.appendChild(pElement);
      listElement.appendChild(buttonElement);
      appUl?.append(listElement);
    }
  } else {
    appUl!.innerHTML = "<li>No apps found in directory</li>";
  }
};

const displayInstalledApps = async () => {
  const appUl = document.querySelector("#installed-apps");
  appUl!.innerHTML = "";
  try {
    await invoke("connect_adb");
    await setConnectedStatus();
  } catch (error) {
    sendAppNotification(
      "Connection Failed",
      `Couldn't connect to host address: ${error}`
    );
  }

  const apkList: PackageInfo[] = await invoke(
    "get_all_third_party_package_info"
  );

  if (apkList.length) {
    apkList.sort((a, b) => a.label.localeCompare(b.label));
    for (const apk of apkList) {
      const listElement = document.createElement("li");
      const pElement = document.createElement("p");
      const buttonElement = document.createElement("fluent-button");
      pElement.textContent = decodeURIComponent(apk.label);
      buttonElement.textContent = "Uninstall";
      buttonElement.addEventListener("click", async () => {
        // await runInstallation(apkPath);
      });

      listElement.appendChild(pElement);
      listElement.appendChild(buttonElement);
      appUl?.append(listElement);
    }
  } else {
    appUl!.innerHTML = "<li>No apps found in directory</li>";
  }
};

const setConnectedStatus = async () => {
  const connStatus = document.getElementById("conn-status");
  const address = await store.get<string>("host-address");
  console.log(address);
  const isConnected: boolean = await invoke("check_if_connected_to_host", {
    hostAddress: address,
  });

  console.log(isConnected);

  connStatus!.textContent = isConnected ? "⚡Connected" : "🔌Disconnected";
};

const setTheme = (theme: Theme) => {
  baseLayerLuminance.setValueFor(
    document.body,
    theme === "dark" ? StandardLuminance.DarkMode : StandardLuminance.LightMode
  );
};

const setInitAddress = async () => {
  editAddressInput.value = (await store.get<string>("host-address")) || "";
};

const setDirectoryValue = async () => {
  const appDir = await store.get<string>("default-app-dir");
  appDirEl!.textContent = appDir as string;
};

// Event Listeners
listen("tauri://file-drop", async (event) => {
  const payload = event.payload as string[];
  setSelectedFiles(payload);
  for (const path of payload) {
    const file = getFileInfo(path);
    fileHandler(file.fileName, file.fileExtension, path);
  }
});

appWindow.onThemeChanged(({ payload }) => {
  setTheme(payload);
});

appWindow.theme().then((theme) => setTheme(theme || "light"));

const themeToggleBtn = document.getElementById("theme-toggler");
themeToggleBtn?.addEventListener("click", async () => {
  const newTheme =
    baseLayerLuminance.getValueFor(document.body) === StandardLuminance.DarkMode
      ? "light"
      : "dark";
  setTheme(newTheme);
});

const appDirEl = document.getElementById("default-dir");
const changeDirBtn = document.getElementById("change-app-dir");
changeDirBtn?.addEventListener("click", async () => {
  // Open a selection dialog for directories
  const selected = await open({
    directory: true,
  });
  if (!selected) return;
  await setDefaultAppDir(selected as string);
  appDirEl!.textContent = selected as string;
  displayApps();
});

const installAllBtn = document.getElementById(
  "install-all-btn"
) as HTMLButtonElement;
const removeAllBtn = document.getElementById(
  "remove-all-btn"
) as HTMLButtonElement;

removeAllBtn?.addEventListener("click", () => {
  setSelectedFiles(null);
  chosenApkEl!.innerHTML = "";
});

installAllBtn?.addEventListener("click", async () => {
  if (!selectedFiles) return;
  for (const file of selectedFiles) {
    await runInstallation(file);
  }
});

const editAddressInput = document.getElementById(
  "edit-field"
) as HTMLInputElement;
const editCancelBtn = document.getElementById("edit-cancel-btn");
const editAddressBtn = document.getElementById("edit-address-btn");

editAddressBtn?.addEventListener("click", async () => {
  if (editAddressBtn.textContent === "Edit") {
    editAddressInput!.disabled = false;
    editAddressBtn!.textContent = "Save";
    editCancelBtn!.hidden = false;
  } else {
    const isAddress = validateAddress(editAddressInput!.value.trim());
    if (isAddress) {
      await setHostAddress(editAddressInput!.value.trim());
      editAddressInput!.disabled = true;
      editAddressBtn!.textContent = "Edit";
      editCancelBtn!.hidden = true;
    }
  }
});

editCancelBtn?.addEventListener("click", async () => {
  const address = await store.get<string>("host-address");
  editAddressInput!.value = address!;
  editAddressInput!.disabled = true;
  editAddressBtn!.textContent = "Edit";
  editCancelBtn!.hidden = true;
});

const dropArea = document.querySelector("#dropzone");
dropArea?.addEventListener("click", async () => {
  // Open a selection dialog for image files
  const selected = await open({
    multiple: true,
    filters: [
      {
        name: "Choose APK",
        extensions: ["apk"],
      },
    ],
  });

  const sel = Array.isArray(selected) ? selected : selected ? [selected] : null;
  setSelectedFiles(sel);
  if (!sel) return;
  sel.forEach(async (path) => {
    const file = getFileInfo(path);

    fileHandler(file.fileName, file.fileExtension, path);
  });
});

window.addEventListener("DOMContentLoaded", () => {
  error = document.getElementById("error");
  chosenApkEl = document.getElementById("chosen-apks");

  error!.innerText = "";

  const CONNECTION_POLLING_INTERVAL = 1000 * 60 * 2;
  setDirectoryValue();
  setInitAddress();

  displayApps();
  displayInstalledApps();
  setSelectedFiles(null);
  setConnectedStatus();
  setInterval(() => {
    setConnectedStatus();
  }, CONNECTION_POLLING_INTERVAL);
});

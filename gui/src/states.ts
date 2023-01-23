import {atom} from "recoil";
import {invoke} from "@tauri-apps/api";

export const topicState = atom({
    key: "topicState",
    default: 0,
});

export const maskUserState = atom({
    key: "maskUserState",
    default: false,
});

export const saveAtState = atom({
    key: "saveAtState",
    default: "",
});

export const currentStep = atom({
    key: "currentStep",
    default: 0,
});

export const archiveResultState = atom({
    key: "archiveResult",
    default: {
        success: true,
        message: "",
    },
});

export const tokenState = atom({
    key: "token",
    default: "",
    effects: [
        ({setSelf, onSet}) => {
            invoke("get_token")
                .then(token => {
                    setSelf(token as string);
                })

            onSet((newValue, _, isReset) => {
                if (isReset || newValue === '') {
                    invoke("del_token")
                } else {
                    invoke("set_token", {token: newValue})
                }
            })
        }
    ]
});

let rateLimitInterval: number | undefined = undefined;
export const rateLimitState = atom({
    key: "rateLimit",
    default: 0,
    effects: [
        ({setSelf, onSet}) => {
            onSet((newValue, oldValue, isReset) => {
                console.log("rateLimitState", newValue, oldValue, isReset);
                if (isReset) {
                    setSelf(0);
                    clearInterval(rateLimitInterval)
                    return;
                }
                if (newValue > oldValue) {
                    clearInterval(rateLimitInterval);
                    console.log("rateLimitState", "start interval", newValue);
                    setSelf(newValue);

                    let i = 1;
                    rateLimitInterval = setInterval(() => {
                        console.log("rateLimitState", "interval", newValue - i);
                        setSelf(newValue - i);
                        if (newValue - i <= 0) {
                            clearInterval(rateLimitInterval);
                        } else {
                            i++;
                        }
                    }, 1000);
                }
            });
        }
    ]
})
import {useRecoilState, useSetRecoilState} from "recoil";
import {currentStep, tokenState} from "../states";
import {Button, Center, Group, Loader, Space, Stack, Text, Textarea} from "@mantine/core";
import {useEffect, useState} from "react";
import {invoke} from "@tauri-apps/api";

export const Login = () => {
    const [loading, setLoading] = useState(false);
    const [opened, setOpened] = useState(false);
    const [token, setToken] = useRecoilState(tokenState);
    const setCurrentStep = useSetRecoilState(currentStep);
    const [OAuthKey, setOAuthKey] = useState("");
    const [keyError, setKeyError] = useState("");
    const enabled = OAuthKey.trim().length > 0;

    const validateToken = async (token: string) => {
        setLoading(true);
        const valid = await invoke("validate_token", {token}) as boolean;
        if (valid) {
            setCurrentStep(1);
        } else {
            setToken('');
        }
        setLoading(false);
    }

    const loginWithOAuthKey = async () => {
        const token = await invoke("token_from_oauth", {oauthKey: OAuthKey}) as string;
        if (token !== '') {
            setToken(token);
        } else {
            console.log("key error");
            setKeyError("无效的授权码");
        }
    }

    useEffect(() => {
        if (token !== '') {
            validateToken(token);
        }
    }, [token]);

    return (
        <>{loading ? <Center pt={130}>
            <Group>
                <Loader/>
                <Text>正在登录...</Text>
            </Group>
        </Center> : (!opened ? <Stack pt={70}>
                <Text align={"center"}>为了存档水源贴子，我们需要您水源账号的只读权限</Text>
                <Button onClick={() => {
                    invoke("open_browser").then(() => {
                        setOpened(true);
                    });
                }}>授权</Button>
            </Stack> : <Stack pt={40}>
                <Text align={"center"}>请在弹出的网页授权后，按照指引将授权码粘贴在下方</Text>
                <Textarea placeholder="授权码" required
                          value={OAuthKey}
                          onChange={ev => setOAuthKey(ev.currentTarget.value)}
                          error={keyError}
                />
                <Space/>
                <Button disabled={!enabled} onClick={() => {
                    loginWithOAuthKey()
                }}>登录</Button>
            </Stack>
        )}</>
    )
}
import {useRecoilState, useSetRecoilState} from "recoil";
import {currentStep, tokenState} from "../states";
import {Button, Center, Group, Loader, Space, Stack, Text, Textarea} from "@mantine/core";
import {useEffect, useState} from "react";
import {loginWithToken, openBrowser, tokenFromOauth} from "../commands";

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
        try {
            await loginWithToken(token);
            setCurrentStep(1);
        } catch (e) {
            setToken('');
        }
        setLoading(false);
    }

    const loginWithOAuthKey = async () => {
        const token = await tokenFromOauth(OAuthKey.replaceAll("\n", ""));
        if (token !== '') {
            setToken(token);
        } else {
            setKeyError("无效的授权码");
        }
    }

    useEffect(() => {
        if (token !== '') {
            validateToken(token).then(_ => {
            });
        }
    }, [token]);

    return (
      <>{loading ?
        <Center pt={130}>
            <Group>
                <Loader/>
                <Text>正在登录...</Text>
            </Group>
        </Center> :
        (!opened ?
            <Stack pt={70}>
                <Text align={"center"}>为了存档水源贴子，我们需要您水源账号的只读权限</Text>
                <Button onClick={() => {
                    openBrowser().then(() => {
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
                    loginWithOAuthKey().then(_ => {
                    })
                }}>登录</Button>
            </Stack>
        )}</>
    )
}
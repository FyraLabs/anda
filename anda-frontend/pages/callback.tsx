import { useHandleSignInCallback } from "@logto/react";
import { useRouter } from "next/router";

const Callback = () => {
  const router = useRouter();
  const { isLoading } = useHandleSignInCallback(() => {
    router.push("/");
  });

  if (isLoading) {
    return <div>Redirecting...</div>;
  }
};

export default Callback;
